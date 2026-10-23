//! `phpev park|unpark|link|unlink|sites` and `phpev hosts sync`.

use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use phpev_core::config::{self, Config};
use phpev_core::layout::Layout;
use phpev_core::sites::{self, Kind};
use phpev_core::{ops, php, shim, version};

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
    if let Some(v) = &php_version {
        let installed = php::installed(&layout);
        if version::best_match(v, installed.iter().map(String::as_str)).is_none() {
            println!("warning: PHP {v} is not installed yet (phpev php install {v})");
        }
    }
    let linked = ops::link_site(&layout, &path, name.as_deref(), php_version)?;
    let config = Config::load(&layout.config_file())?;
    println!(
        "{} http://{}.{}  →  {}",
        if linked.replaced { "Re-linked" } else { "Linked" },
        linked.name,
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
    println!("Asking for administrator rights to update the hosts file…");
    let n = ops::write_hosts(config, include)?;
    println!("Hosts file updated ({n} sites).");
    Ok(())
}
