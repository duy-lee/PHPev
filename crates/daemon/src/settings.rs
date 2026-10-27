//! What the Settings page reads and changes. Blocking; call through
//! `spawn_blocking`. Every value is checked here, never trusted from the page.

use std::io;
use std::path::Path;

use phpev_core::api::{GeneralSettings, SettingsInfo, StorageItem};
use phpev_core::config::{self, Config, Ports};
use phpev_core::layout::Layout;
use phpev_core::{autostart, ca, doctor, elevate, ops, sites};

pub fn info(layout: &Layout) -> io::Result<SettingsInfo> {
    let config = Config::load(&layout.config_file())?;
    let ca_trusted = match ca::stored_der(layout) {
        Some(der) => ca::is_trusted(&der).unwrap_or(false),
        None => false,
    };
    Ok(SettingsInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        autostart: autostart::is_enabled().unwrap_or(false),
        tray: config.tray,
        theme: config.ui.theme.clone().unwrap_or_else(|| "system".into()),
        check_updates: config.ui.check_updates(),
        tld: config.tld.clone(),
        parked: config.parked.iter().map(|p| p.display().to_string()).collect(),
        ports: config.ports.clone(),
        ca_exists: ca::exists(layout),
        ca_trusted,
        secure_sites: sites::all(&config)
            .iter()
            .filter(|s| s.secure)
            .map(|s| s.name.clone())
            .collect(),
        defender_excluded: config.defender_excluded,
        checks: doctor::run(layout, &config).0,
        home: layout.root().display().to_string(),
        config_file: layout.config_file().display().to_string(),
    })
}

/// Saves what `r` names; the tray icon follows at once, and turning the
/// update check on runs it if one is due.
pub fn set_general(layout: &Layout, r: &GeneralSettings) -> io::Result<()> {
    let theme = match r.theme.as_deref() {
        None => None,
        Some("system") => Some(None),
        Some(t @ ("light" | "dark")) => Some(Some(t.to_owned())),
        Some(other) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unknown theme `{other}`"),
            ))
        }
    };
    if let Some(id) = &r.editor {
        if !phpev_core::editors::installed().iter().any(|e| &e.id == id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("`{id}` is not an installed editor"),
            ));
        }
    }
    if let Some(on) = r.autostart {
        autostart::set(layout, on)?;
    }
    if r.tray.is_some() || theme.is_some() || r.editor.is_some() || r.check_updates.is_some() {
        config::update(&layout.config_file(), |c| {
            if let Some(id) = &r.editor {
                c.ui.editor = Some(id.clone());
            }
            if let Some(on) = r.tray {
                c.tray = on;
            }
            if let Some(theme) = theme {
                c.ui.theme = theme;
            }
            if let Some(on) = r.check_updates {
                // Unset means on.
                c.ui.check_updates = (!on).then_some(false);
            }
        })?;
    }
    if let Some(on) = r.tray {
        crate::tray::set_visible(on);
    }
    Ok(())
}

/// Ports must differ from each other, and the ones that change must be free.
pub fn check_ports(current: &Ports, wanted: &Ports) -> io::Result<()> {
    let all = [
        ("HTTP", current.http, wanted.http),
        ("HTTPS", current.https, wanted.https),
        ("MariaDB", current.mariadb, wanted.mariadb),
        ("the dashboard", current.dashboard, wanted.dashboard),
    ];
    for (i, (what, _, port)) in all.iter().enumerate() {
        if *port == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("choose a port for {what} (1-65535)"),
            ));
        }
        if let Some((other, _, _)) = all[..i].iter().find(|(_, _, p)| p == port) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{other} and {what} cannot share port {port}"),
            ));
        }
    }
    for (what, old, new) in all {
        if old != new {
            crate::ensure_port_free(new, what)?;
        }
    }
    Ok(())
}

/// Sizes of the folders PHPev fills (can take a moment for large databases).
pub fn storage(layout: &Layout) -> Vec<StorageItem> {
    let item = |id: &str, path: &Path, clearable: bool| StorageItem {
        id: id.into(),
        path: path.display().to_string(),
        bytes: ops::dir_size(path),
        clearable,
    };
    vec![
        item("php", &layout.php_dir(), false),
        item("mariadb", &layout.mariadb_dir(), false),
        item("databases", &layout.mariadb_data_dir(), false),
        item("composer", &layout.composer_dir(), false),
        item("downloads", &layout.downloads_dir(), true),
        item("logs", &layout.logs_dir(), true),
    ]
}

/// Empties the download cache, or the logs: older rotated files are deleted
/// and current ones emptied (PHP, MariaDB and PHPev keep writing to them).
/// Returns how many bytes were freed.
pub fn clean(layout: &Layout, id: &str) -> io::Result<u64> {
    let dir = match id {
        "downloads" => layout.downloads_dir(),
        "logs" => layout.logs_dir(),
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("`{other}` cannot be cleaned up"),
            ))
        }
    };
    let before = ops::dir_size(&dir);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(0);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Files in use (a download running now) stay; the rest goes.
        let _ = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else if id == "logs" && is_current_log(&path) {
            std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .and_then(|f| f.set_len(0))
        } else {
            std::fs::remove_file(&path)
        };
    }
    Ok(before.saturating_sub(ops::dir_size(&dir)))
}

/// `daemon.log`, `php-8.4.1.log`, `mariadb-11.4.err`; not `daemon.log.1`.
fn is_current_log(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("log") || e.eq_ignore_ascii_case("err"))
}

/// Excludes the PHPev home from Defender scanning (Windows asks for permission).
pub fn defender_exclude_home(layout: &Layout) -> io::Result<()> {
    let home = layout.root().to_string_lossy().into_owned();
    match elevate::run_elevated(&elevate::helper_path()?, &[elevate::CMD_DEFENDER_EXCLUDE, &home])? {
        0 => config::update(&layout.config_file(), |c| c.defender_excluded = true),
        code => Err(io::Error::other(format!(
            "Windows did not add the exclusion (exit code {code})"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_must_differ() {
        let current = Ports::default();
        let wanted = Ports {
            https: 80,
            ..Ports::default()
        };
        let e = check_ports(&current, &wanted).unwrap_err();
        assert!(e.to_string().contains("cannot share port 80"), "{e}");
    }

    #[test]
    fn port_zero_is_refused() {
        let wanted = Ports {
            mariadb: 0,
            ..Ports::default()
        };
        assert!(check_ports(&Ports::default(), &wanted).is_err());
    }

    #[test]
    fn unchanged_ports_are_not_probed() {
        // The current ports are in use by PHPev itself; keeping them is fine.
        assert!(check_ports(&Ports::default(), &Ports::default()).is_ok());
    }

    #[test]
    fn current_logs() {
        assert!(is_current_log(Path::new(r"C:\x\logs\daemon.log")));
        assert!(is_current_log(Path::new(r"C:\x\logs\mariadb-11.4.err")));
        assert!(!is_current_log(Path::new(r"C:\x\logs\daemon.log.1")));
    }
}
