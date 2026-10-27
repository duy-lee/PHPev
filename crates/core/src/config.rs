//! `config.toml` model and atomic persistence.
//!
//! Only the daemon writes this file at runtime; `phpev setup`
//! writes the initial file once, before any daemon exists.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Default PHP version (e.g. `"8.4"`) when no project-specific one applies.
    pub php: Option<String>,
    /// Active MariaDB major version (e.g. `"11.4"`).
    pub mariadb: Option<String>,
    /// TLD used for parked/linked sites.
    pub tld: String,
    pub ports: Ports,
    /// Directories whose sub-folders each become `<name>.<tld>`.
    pub parked: Vec<PathBuf>,
    /// Individually linked sites.
    pub sites: Vec<Site>,
    /// Set once `phpev doctor --defender-exclude` succeeded. Non-admin users
    /// cannot read Defender's exclusion list, so PHPev remembers it here.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub defender_excluded: bool,
    /// Show the notification-area icon (on unless turned off).
    #[serde(skip_serializing_if = "is_true")]
    pub tray: bool,
    /// Project folders PHPev excluded from Defender scanning that are not
    /// linked sites (those carry `Site::defender_excluded`), e.g. parked ones.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub defender_excluded_dirs: Vec<PathBuf>,
    /// Dashboard choices.
    #[serde(skip_serializing_if = "Ui::is_default")]
    pub ui: Ui,
}

fn is_true(b: &bool) -> bool {
    *b
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ui {
    /// The code editor "Open in editor" uses (`vscode`, `phpstorm`, …), once chosen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor: Option<String>,
    /// `light` or `dark`; the Windows setting when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Look for a newer PHPev release once a day (on unless turned off).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_updates: Option<bool>,
}

impl Ui {
    pub fn check_updates(&self) -> bool {
        self.check_updates != Some(false)
    }

    fn is_default(&self) -> bool {
        *self == Ui::default()
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .trim_end_matches('\\')
        .eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches('\\'))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ports {
    pub http: u16,
    pub https: u16,
    pub mariadb: u16,
    /// Dashboard on 127.0.0.1.
    pub dashboard: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Site {
    pub name: String,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub php: Option<String>,
    #[serde(default)]
    pub secure: bool,
    /// A Microsoft Defender exclusion was added for this folder by
    /// `phpev import` (non-admin users cannot read the exclusion list).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub defender_excluded: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            php: None,
            mariadb: None,
            tld: "localhost".into(),
            ports: Ports::default(),
            parked: Vec::new(),
            sites: Vec::new(),
            defender_excluded: false,
            tray: true,
            defender_excluded_dirs: Vec::new(),
            ui: Ui::default(),
        }
    }
}

impl Default for Ports {
    fn default() -> Self {
        Self {
            http: 80,
            https: 443,
            mariadb: 3306,
            dashboard: 7600,
        }
    }
}

impl Config {
    /// True if PHPev added a Defender exclusion for project folder `dir`.
    pub fn is_defender_excluded(&self, dir: &Path) -> bool {
        self.sites
            .iter()
            .any(|s| s.defender_excluded && same_path(&s.path, dir))
            || self.defender_excluded_dirs.iter().any(|d| same_path(d, dir))
    }

    /// Remembers a Defender exclusion added for project folder `dir`.
    pub fn mark_defender_excluded(&mut self, dir: &Path) {
        match self.sites.iter_mut().find(|s| same_path(&s.path, dir)) {
            Some(site) => site.defender_excluded = true,
            None if !self.defender_excluded_dirs.iter().any(|d| same_path(d, dir)) => {
                self.defender_excluded_dirs.push(dir.to_owned())
            }
            None => {}
        }
    }

    /// Loads the config; a missing file yields the defaults.
    pub fn load(path: &Path) -> io::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// Writes the config atomically (write `<file>.tmp`, then rename over the
    /// original) and refreshes the shims' cache next to it.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let text = toml::to_string_pretty(self).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        write_atomic(path, text.as_bytes())?;
        crate::shim::write_cache(path, self)
    }
}

/// Name of the cross-process lock every config.toml writer holds.
const LOCK_NAME: &str = "PHPev.Config";

/// Load → modify → save under the cross-process config lock, so the daemon and
/// a CLI running while the daemon is down never overwrite each other (§10).
pub fn update<T>(path: &Path, f: impl FnOnce(&mut Config) -> T) -> io::Result<T> {
    let _guard = crate::winsync::lock(LOCK_NAME, std::time::Duration::from_secs(10))?;
    let mut config = Config::load(path)?;
    let out = f(&mut config);
    config.save(path)?;
    Ok(out)
}

/// Replaces `path` with `data` so readers never observe a half-written file.
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, data)?;
    // std::fs::rename uses MoveFileExW(MOVEFILE_REPLACE_EXISTING) on Windows.
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_gives_defaults() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn partial_ports_keep_other_defaults() {
        let cfg: Config = toml::from_str("[ports]\nhttp = 8080\n").unwrap();
        assert_eq!(
            cfg.ports,
            Ports {
                http: 8080,
                ..Ports::default()
            }
        );
    }

    #[test]
    fn round_trip() {
        let cfg = Config {
            php: Some("8.4".into()),
            sites: vec![Site {
                name: "blog".into(),
                path: r"C:\code\blog".into(),
                php: None,
                secure: true,
                defender_excluded: false,
            }],
            ..Config::default()
        };
        let text = toml::to_string_pretty(&cfg).unwrap();
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), cfg);
    }

    #[test]
    fn save_then_load() {
        let dir = std::env::temp_dir().join(format!("phpev-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("config.toml");
        let cfg = Config {
            mariadb: Some("11.4".into()),
            ..Config::default()
        };
        cfg.save(&file).unwrap();
        cfg.save(&file).unwrap(); // overwriting an existing file must work too
        assert_eq!(Config::load(&file).unwrap(), cfg);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
