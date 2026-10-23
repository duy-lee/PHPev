//! `phpev secure|unsecure`, `phpev ca status|uninstall`, `phpev tld` (M7).

use std::io;
use std::path::Path;
use std::process::ExitCode;

use clap::Subcommand;
use phpev_core::ca::{self, Ca};
use phpev_core::config::{self, Config, Site};
use phpev_core::ipc::{self, Request};
use phpev_core::layout::Layout;
use phpev_core::sites;

use crate::daemon;
use crate::sites::{absolute, auto_hosts, write_hosts};

#[derive(Subcommand)]
pub enum CaCommand {
    /// Show whether the PHPev CA exists and is trusted
    Status,
    /// Remove the CA from Windows and PHP, delete its key, and turn HTTPS off for every site
    Uninstall,
}

/// The site named `name`, or the one whose folder is the current directory.
fn find_site(config: &Config, name: Option<&str>) -> io::Result<sites::Site> {
    let all = sites::all(config);
    let found = match name {
        Some(n) => all.into_iter().find(|s| s.name == n),
        None => {
            let cwd = absolute(None)?;
            all.into_iter().find(|s| same(&s.path, &cwd))
        }
    };
    found.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            match name {
                Some(n) => format!("no site named `{n}` (see `phpev sites`)"),
                None => "the current folder is not a site; pass a name or run `phpev link`".to_owned(),
            },
        )
    })
}

fn same(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

/// Creates the CA if needed and makes Windows and PHP trust it.
fn ensure_trusted_ca(layout: &Layout, tld: &str) -> io::Result<Ca> {
    let ca = Ca::load_or_create(layout, tld)?;
    if !ca::is_trusted(ca.der())? {
        println!(
            "Windows will now ask you to trust \"PHPev Local Development CA\".\n  \
             It can only sign *.localhost, *.test, *.local{} — never real websites.",
            if ["localhost", "test", "local"].contains(&tld) {
                String::new()
            } else {
                format!(" and *.{tld}")
            }
        );
        ca::trust(ca.der())?;
        println!("CA trusted (CurrentUser\\Root).");
    }
    // PHP (curl/openssl) does not use the Windows store: add it to cacert.pem.
    ca::update_php_bundle(layout, Some(ca.pem()))?;
    Ok(ca)
}

pub fn secure(name: Option<String>) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    let config = Config::load(&layout.config_file())?;
    let site = find_site(&config, name.as_deref())?;
    ensure_trusted_ca(&layout, &config.tld)?;

    // A parked site gets a link entry to carry the flag.
    config::update(&layout.config_file(), |c| {
        match c.sites.iter_mut().find(|s| s.name == site.name) {
            Some(s) => s.secure = true,
            None => c.sites.push(Site {
                name: site.name.clone(),
                path: site.path.clone(),
                php: None,
                secure: true,
                defender_excluded: false,
            }),
        }
    })?;

    if ipc::is_running() {
        match daemon::request(&Request::EnableHttps) {
            Ok(msg) => println!("{msg}"),
            Err(e) => println!("warning: {e} (see `phpev status`)"),
        }
    } else {
        println!("HTTPS starts with the daemon (`phpev start`).");
    }
    let port = if config.ports.https == 443 {
        String::new()
    } else {
        format!(":{}", config.ports.https)
    };
    let url = format!("https://{}{port}", site.host(&config.tld));
    println!("Secured {url}  (http:// now redirects here)");
    if site.path.join(".env").is_file() {
        println!("Tip: set APP_URL={url} in {}", site.path.join(".env").display());
    }
    auto_hosts(&layout);
    Ok(ExitCode::SUCCESS)
}

pub fn unsecure(name: Option<String>) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    let config = Config::load(&layout.config_file())?;
    let site = find_site(&config, name.as_deref())?;
    let changed = config::update(&layout.config_file(), |c| {
        c.sites
            .iter_mut()
            .find(|s| s.name == site.name)
            .map(|s| std::mem::replace(&mut s.secure, false))
    })?;
    if changed == Some(true) {
        println!("HTTPS off for http://{}", site.host(&config.tld));
    } else {
        println!("{} was not secured", site.host(&config.tld));
    }
    Ok(ExitCode::SUCCESS)
}

pub fn ca(command: CaCommand) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    match command {
        CaCommand::Status => {
            match ca::stored_der(&layout) {
                None => println!("No PHPev CA yet (created by `phpev secure`)."),
                Some(der) => {
                    let trusted = ca::is_trusted(&der)?;
                    println!("CA:      {}", layout.certs_dir().join("ca.crt.pem").display());
                    println!(
                        "Trusted: {}",
                        if trusted {
                            "yes (CurrentUser\\Root)"
                        } else {
                            "NO — run `phpev secure <site>`"
                        }
                    );
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        CaCommand::Uninstall => {
            if let Some(der) = ca::stored_der(&layout) {
                if ca::is_trusted(&der)? {
                    println!("Windows will ask you to confirm removing the PHPev CA…");
                    ca::untrust(&der)?;
                    println!("Removed from CurrentUser\\Root.");
                } else {
                    println!("The CA was not in CurrentUser\\Root.");
                }
            }
            ca::update_php_bundle(&layout, None)?;
            for f in ["ca.key.pem", "ca.crt.pem", "ca.constraints"] {
                let _ = std::fs::remove_file(layout.certs_dir().join(f));
            }
            config::update(&layout.config_file(), |c| {
                c.sites.iter_mut().for_each(|s| s.secure = false)
            })?;
            println!("CA deleted and HTTPS turned off for all sites.");
            if ipc::is_running() {
                println!("Restart PHPev to stop the HTTPS listener: phpev stop && phpev start");
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Shows or changes the TLD. Leaving `localhost` needs hosts entries; going
/// back removes them. A custom TLD changes the CA's name constraints.
pub fn tld(new: Option<String>) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    let config = Config::load(&layout.config_file())?;
    let Some(new) = new else {
        println!("{}", config.tld);
        return Ok(ExitCode::SUCCESS);
    };
    let new = new.trim_start_matches('.').to_ascii_lowercase();
    if new.is_empty()
        || new.len() > 63
        || !new
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a TLD is one label: a-z, 0-9, -",
        ));
    }
    if new == config.tld {
        println!("TLD is already .{new}");
        return Ok(ExitCode::SUCCESS);
    }
    if new == "local" {
        println!("note: .local is also used by mDNS (printers, LAN devices); PHPev writes both 127.0.0.1 and ::1 entries to keep lookups fast.");
    }
    let old = config.tld.clone();
    config::update(&layout.config_file(), |c| c.tld = new.clone())?;
    println!("TLD changed: .{old} → .{new}");

    let updated = Config::load(&layout.config_file())?;
    if new == "localhost" {
        if let Err(e) = write_hosts(&updated, false) {
            println!("warning: could not remove PHPev hosts entries ({e})");
        }
    } else {
        auto_hosts(&layout);
    }
    if ca::exists(&layout) && updated.sites.iter().any(|s| s.secure) {
        // A non-built-in TLD needs a CA certificate permitting it.
        ensure_trusted_ca(&layout, &new)?;
        println!("Restart PHPev so HTTPS uses the new names: phpev stop && phpev start");
    }
    Ok(ExitCode::SUCCESS)
}
