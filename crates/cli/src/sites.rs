//! `phpev park|unpark|link|unlink|sites` and `phpev hosts sync`.

use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use phpev_core::config::{self, Config, Site};
use phpev_core::layout::Layout;
use phpev_core::sites::{self, Kind};
use phpev_core::{elevate, hosts, php, shim, version};

/// Absolute, normalized form of `dir` (default: the current directory).
pub(crate) fn absolute(dir: Option<PathBuf>) -> io::Result<PathBuf> {
    let dir = match dir {
        Some(d) => d,
        None => std::env::current_dir()?,
    };
    let real = std::fs::canonicalize(&dir).map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", dir.display())))?;
    if !real.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a directory", dir.display()),
        ));
    }
    Ok(strip_verbatim(real))
}

/// `canonicalize` returns `\\?\C:\…`; config and PHP want the plain form.
fn strip_verbatim(p: PathBuf) -> PathBuf {
    match p.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => p,
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

pub fn park(dir: Option<PathBuf>) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    let dir = absolute(dir)?;
    let added = config::update(&layout.config_file(), |c| {
        let new = !c.parked.iter().any(|p| same_path(p, &dir));
        if new {
            c.parked.push(dir.clone());
        }
        new
    })?;
    println!("{} {}", if added { "Parked" } else { "Already parked:" }, dir.display());
    let config = Config::load(&layout.config_file())?;
    let found: Vec<_> = sites::all(&config)
        .into_iter()
        .filter(|s| s.kind == Kind::Parked && s.path.starts_with(&dir))
        .collect();
    if found.is_empty() {
        println!(
            "No sub-folders yet; each new folder becomes http://<folder>.{}",
            config.tld
        );
    }
    for s in found {
        println!("  http://{}  →  {}", s.host(&config.tld), s.path.display());
    }
    auto_hosts(&layout);
    Ok(ExitCode::SUCCESS)
}

pub fn unpark(dir: Option<PathBuf>) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    let dir = absolute(dir)?;
    let removed = config::update(&layout.config_file(), |c| {
        let before = c.parked.len();
        c.parked.retain(|p| !same_path(p, &dir));
        before != c.parked.len()
    })?;
    println!(
        "{} {}",
        if removed { "Unparked" } else { "Was not parked:" },
        dir.display()
    );
    if removed {
        auto_hosts(&layout);
    }
    Ok(ExitCode::SUCCESS)
}

pub fn link(name: Option<String>, php_version: Option<String>) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    let path = absolute(None)?;
    let folder = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match name {
        Some(n) => sites::normalize_name(&n).filter(|norm| *norm == n).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("`{n}` is not a valid site name (use a-z, 0-9, -)"),
            )
        })?,
        None => sites::normalize_name(&folder).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot derive a site name from this folder; pass one: phpev link <name>",
            )
        })?,
    };
    if let Some(v) = &php_version {
        let installed = php::installed(&layout);
        if version::best_match(v, installed.iter().map(String::as_str)).is_none() {
            println!("warning: PHP {v} is not installed yet (phpev php install {v})");
        }
    }
    let replaced = config::update(&layout.config_file(), |c| {
        let previous = c.sites.iter().find(|s| s.name == name).cloned();
        c.sites.retain(|s| s.name != name);
        // Re-linking the same folder keeps its HTTPS and Defender flags.
        let same = previous.as_ref().filter(|p| same_path(&p.path, &path));
        c.sites.push(Site {
            name: name.clone(),
            path: path.clone(),
            php: php_version.clone(),
            secure: same.is_some_and(|p| p.secure),
            defender_excluded: same.is_some_and(|p| p.defender_excluded),
        });
        previous.is_some()
    })?;
    let config = Config::load(&layout.config_file())?;
    println!(
        "{} http://{name}.{}  →  {}",
        if replaced { "Re-linked" } else { "Linked" },
        config.tld,
        path.display()
    );
    auto_hosts(&layout);
    Ok(ExitCode::SUCCESS)
}

pub fn unlink(name: Option<String>) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    let cwd = absolute(None)?;
    let removed = config::update(&layout.config_file(), |c| {
        let pos = match &name {
            Some(n) => c.sites.iter().position(|s| &s.name == n),
            None => c.sites.iter().position(|s| same_path(&s.path, &cwd)),
        };
        pos.map(|i| c.sites.remove(i))
    })?;
    match removed {
        Some(s) => {
            println!("Unlinked {} ({})", s.name, s.path.display());
            auto_hosts(&layout);
        }
        None => println!(
            "No linked site {}",
            name.unwrap_or_else(|| format!("for {}", cwd.display()))
        ),
    }
    Ok(ExitCode::SUCCESS)
}

pub fn list() -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    let config = Config::load(&layout.config_file())?;
    let all = sites::all(&config);
    if all.is_empty() {
        println!("No sites. Run `phpev park <dir>` or `phpev link` inside a project.");
    }
    let port = if config.ports.http == 80 {
        String::new()
    } else {
        format!(":{}", config.ports.http)
    };
    for s in &all {
        let php = match shim::resolve(layout.root(), &s.path) {
            Ok(r) => format!("PHP {}", r.version),
            Err(_) => "PHP ?".to_owned(),
        };
        let kind = if s.kind == Kind::Linked { "linked" } else { "parked" };
        let excluded = config.sites.iter().any(|c| c.name == s.name && c.defender_excluded);
        let defender = if excluded { "  (Defender exclusion)" } else { "" };
        println!(
            "http://{}{port}  {kind}  {php}  {}{defender}",
            s.host(&config.tld),
            s.path.display()
        );
    }
    if !config.parked.is_empty() {
        println!("\nParked directories:");
        for p in &config.parked {
            println!("  {}", p.display());
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Writes every current site into the hosts file (127.0.0.1 and ::1) through UAC.
pub fn hosts_sync() -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    let config = Config::load(&layout.config_file())?;
    write_hosts(&config, true)?;
    Ok(ExitCode::SUCCESS)
}

/// With a TLD other than `localhost` the browser needs hosts entries, so site
/// changes rewrite them right away (one UAC prompt; decided 2026-10-23). A
/// declined prompt is a warning, not an error: the config change stands.
pub(crate) fn auto_hosts(layout: &Layout) {
    let Ok(config) = Config::load(&layout.config_file()) else {
        return;
    };
    if config.tld == "localhost" {
        return;
    }
    if let Err(e) = write_hosts(&config, true) {
        println!("warning: hosts file not updated ({e}); run `phpev hosts sync` later");
    }
}

/// Rewrites the PHPev block of the hosts file with every site (`include`) or
/// removes it (`!include`).
pub(crate) fn write_hosts(config: &Config, include: bool) -> io::Result<()> {
    let names: Vec<String> = if include {
        sites::all(config).iter().map(|s| s.host(&config.tld)).collect()
    } else {
        Vec::new()
    };
    if let Some(bad) = names.iter().find(|h| !hosts::is_valid_host(h)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid hostname {bad} (check `tld` in config.toml)"),
        ));
    }
    println!(
        "Asking for administrator rights to update the hosts file ({} sites)…",
        names.len()
    );
    let mut args = vec![elevate::CMD_HOSTS_SYNC];
    args.extend(names.iter().map(String::as_str));
    let code = elevate::run_elevated(&elevate::helper_path()?, &args)?;
    if code != 0 {
        return Err(io::Error::other(format!(
            "{} failed with exit code {code}",
            elevate::HELPER_EXE
        )));
    }
    println!("Hosts file updated.");
    Ok(())
}
