//! Operations shared by the CLI and the daemon's API (one
//! handling layer, many UIs). Nothing here prints; progress and notes go to a
//! [`Report`], which the CLI renders on the terminal and the daemon turns into
//! a job the dashboard follows.

use std::io;
use std::path::{Path, PathBuf};

use crate::ca::{self, Ca};
use crate::config::{self, Config, Site};
use crate::layout::Layout;
use crate::net::Http;
use crate::{clock, elevate, hosts, mariadb, php, sites, version};

/// Receives progress from long operations.
pub trait Report {
    /// A step or note, e.g. "wrote php.ini".
    fn info(&mut self, msg: &str);
    /// Download progress for `label`.
    fn progress(&mut self, label: &str, done: u64, total: Option<u64>);
}

/// Discards everything (for callers that only want the result).
pub struct Quiet;

impl Report for Quiet {
    fn info(&mut self, _: &str) {}
    fn progress(&mut self, _: &str, _: u64, _: Option<u64>) {}
}

fn not_found(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, msg)
}

// ---------------------------------------------------------------- PHP

/// Installs PHP `wanted` (`8.4` or `8.4.27`) with VC++ runtime, CA bundle and
/// php.ini; becomes the default if none is set. Returns the installed version.
pub fn install_php(http: &Http, layout: &Layout, wanted: &str, r: &mut dyn Report) -> io::Result<String> {
    layout.create_dirs()?;
    let releases = php::available(http)?;
    let release = php::resolve(&releases, wanted)?;
    r.info(&format!("PHP {} ({}, NTS x64)", release.version, release.toolset));

    if !php::vc_runtime_ok(&release.toolset) {
        r.info("Visual C++ runtime missing or too old — installing it (Windows will ask for permission)");
        php::install_vc_runtime(http, layout, &mut |d, t| r.progress("vc_redist.x64.exe", d, t))?;
    }
    if php::ensure_cacert(http, layout, &mut |d, t| r.progress("cacert.pem", d, t))? {
        // A fresh bundle must carry the PHPev CA again, if HTTPS is in use.
        if ca::exists(layout) {
            let tld = Config::load(&layout.config_file())?.tld;
            let authority = Ca::load_or_create(layout, &tld)?;
            ca::update_php_bundle(layout, Some(authority.pem()))?;
        }
    }
    match php::install(http, layout, release, &mut |d, t| r.progress(&release.zip, d, t))? {
        Some(stats) => r.info(&format!(
            "unpacked {} files, skipped {} not needed",
            stats.files, stats.skipped
        )),
        None => r.info("already installed"),
    }
    if php::write_php_ini(layout, &release.version)? {
        r.info("wrote php.ini");
    }
    let modules = php::verify(layout, &release.version)?;
    r.info(&format!("verified: {} extensions loaded", modules.len()));
    let became_default = config::update(&layout.config_file(), |c| {
        c.php.is_none().then(|| c.php = Some(release.line.clone())).is_some()
    })?;
    if became_default {
        r.info(&format!("set as default PHP ({})", release.line));
    }
    Ok(release.version.clone())
}

/// Sets the default PHP (a line like `8.4` or an exact version). Returns the
/// installed version it currently resolves to.
pub fn set_default_php(layout: &Layout, wanted: &str) -> io::Result<String> {
    let installed = php::installed(layout);
    let found = version::best_match(wanted, installed.iter().map(String::as_str))
        .ok_or_else(|| not_found(format!("PHP {wanted} is not installed — install it first")))?
        .to_owned();
    config::update(&layout.config_file(), |c| c.php = Some(wanted.to_owned()))?;
    Ok(found)
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Extension {
    pub name: String,
    /// Loaded according to `php -m`.
    pub loaded: bool,
    /// Ships as `ext\php_<name>.dll`, so it can be switched in php.ini.
    pub toggleable: bool,
}

/// Every extension of an installed version: loaded ones and shipped DLLs.
pub fn extensions(layout: &Layout, version: &str) -> io::Result<Vec<Extension>> {
    let loaded: Vec<String> = php::modules(layout, version)?
        .iter()
        .map(|m| m.to_ascii_lowercase())
        .collect();
    let mut dlls: Vec<String> = std::fs::read_dir(php::version_dir(layout, version).join("ext"))?
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().into_string().ok()?;
            Some(n.strip_prefix("php_")?.strip_suffix(".dll")?.to_ascii_lowercase())
        })
        .collect();
    dlls.sort();
    let mut out: Vec<Extension> = dlls
        .iter()
        .map(|d| Extension {
            name: d.clone(),
            loaded: loaded.iter().any(|l| l == d || (d == "opcache" && l == "zend opcache")),
            toggleable: true,
        })
        .collect();
    for l in &loaded {
        let name = if l == "zend opcache" {
            "opcache".to_owned()
        } else {
            l.clone()
        };
        if !out.iter().any(|e| e.name == name) {
            out.push(Extension {
                name,
                loaded: true,
                toggleable: false,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Switches a shipped extension on or off in the version's php.ini and checks
/// PHP still starts. Running workers keep the old settings until restarted.
pub fn set_extension(layout: &Layout, version: &str, ext: &str, enabled: bool) -> io::Result<()> {
    if !php::has_extension_dll(layout, version, ext) {
        return Err(not_found(format!("PHP {version} does not ship the `{ext}` extension")));
    }
    if enabled {
        php::enable_extension(layout, version, ext)?;
    } else {
        php::disable_extension(layout, version, ext)?;
    }
    php::modules(layout, version).map(|_| ())
}

// ---------------------------------------------------------------- MariaDB

/// Installs the latest release of a MariaDB major and initializes its datadir;
/// becomes the active major if none is set. Returns the installed version.
pub fn install_mariadb(http: &Http, layout: &Layout, wanted: &str, r: &mut dyn Report) -> io::Result<String> {
    layout.create_dirs()?;
    let major = mariadb::major_of(wanted);
    let majors = mariadb::majors(http)?;
    let info = majors.iter().find(|m| m.release_id == major).ok_or_else(|| {
        let ids: Vec<&str> = majors.iter().map(|m| m.release_id.as_str()).collect();
        not_found(format!(
            "MariaDB {major} is not a stable release (available: {})",
            ids.join(", ")
        ))
    })?;
    if info.is_eol(&clock::today()) {
        r.info(&format!(
            "warning: MariaDB {major} reached end of life on {}; it gets no security fixes",
            info.release_eol_date.as_deref().unwrap_or("?")
        ));
    }
    let release = mariadb::latest(http, &major)?;
    if wanted != major && wanted != release.version {
        return Err(not_found(format!(
            "only the latest {major} release ({}) can be installed",
            release.version
        )));
    }
    r.info(&format!("MariaDB {}", release.version));
    let label = format!("mariadb-{}-winx64.zip", release.version);
    match mariadb::install(http, layout, &release, &mut |d, t| r.progress(&label, d, t))? {
        Some(stats) => r.info(&format!(
            "unpacked {} files, skipped {} not needed",
            stats.files, stats.skipped
        )),
        None => r.info("already installed"),
    }
    if mariadb::init_data_dir(layout, &release.version)? {
        r.info(&format!(
            "initialized data directory {}",
            mariadb::data_dir(layout, &major).display()
        ));
    }
    let became_active = config::update(&layout.config_file(), |c| {
        c.mariadb.is_none().then(|| c.mariadb = Some(major.clone())).is_some()
    })?;
    if became_active {
        r.info(&format!("set as active MariaDB ({major})"));
    }
    Ok(release.version)
}

/// Makes `wanted`'s major the active one. Returns (installed version, previous major).
pub fn set_active_mariadb(layout: &Layout, wanted: &str) -> io::Result<(String, Option<String>)> {
    let major = mariadb::major_of(wanted);
    let version = mariadb::installed_for_major(layout, &major)
        .ok_or_else(|| not_found(format!("MariaDB {major} is not installed")))?;
    let previous = config::update(&layout.config_file(), |c| c.mariadb.replace(major.clone()))?;
    Ok((version, previous.filter(|p| *p != major)))
}

// ---------------------------------------------------------------- sites

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

pub struct Linked {
    pub name: String,
    pub replaced: bool,
}

/// Links `dir` as a site (name: given or from the folder). Re-linking the same
/// folder keeps its HTTPS and Defender flags. `dir` must be absolute.
pub fn link_site(layout: &Layout, dir: &Path, name: Option<&str>, php_version: Option<String>) -> io::Result<Linked> {
    if !dir.is_absolute() || !dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a folder", dir.display()),
        ));
    }
    let folder = dir
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match name {
        Some(n) => sites::normalize_name(n).filter(|norm| norm == n).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("`{n}` is not a valid site name (use a-z, 0-9, -)"),
            )
        })?,
        None => sites::normalize_name(&folder).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot derive a site name from this folder; give one",
            )
        })?,
    };
    let replaced = config::update(&layout.config_file(), |c| {
        let previous = c.sites.iter().find(|s| s.name == name).cloned();
        c.sites.retain(|s| s.name != name);
        let same = previous.as_ref().filter(|p| same_path(&p.path, dir));
        c.sites.push(Site {
            name: name.clone(),
            path: dir.to_owned(),
            php: php_version.clone(),
            secure: same.is_some_and(|p| p.secure),
            defender_excluded: same.is_some_and(|p| p.defender_excluded),
        });
        previous.is_some()
    })?;
    Ok(Linked { name, replaced })
}

/// Removes a linked site by name. Returns it, if it existed.
pub fn unlink_site(layout: &Layout, name: &str) -> io::Result<Option<Site>> {
    config::update(&layout.config_file(), |c| {
        c.sites.iter().position(|s| s.name == name).map(|i| c.sites.remove(i))
    })
}

/// Pins (or with `None` unpins) the PHP version of a site; a parked site gets
/// a link entry to carry it.
pub fn set_site_php(layout: &Layout, name: &str, php_version: Option<String>) -> io::Result<()> {
    let site = find_site(layout, name)?;
    config::update(&layout.config_file(), |c| {
        match c.sites.iter_mut().find(|s| s.name == name) {
            Some(s) => s.php = php_version.clone(),
            None => c.sites.push(Site {
                name: site.name.clone(),
                path: site.path.clone(),
                php: php_version.clone(),
                secure: false,
                defender_excluded: false,
            }),
        }
    })
}

pub fn find_site(layout: &Layout, name: &str) -> io::Result<sites::Site> {
    let config = Config::load(&layout.config_file())?;
    sites::all(&config)
        .into_iter()
        .find(|s| s.name == name)
        .ok_or_else(|| not_found(format!("no site named `{name}`")))
}

/// Creates the CA if needed and makes Windows (CurrentUser\Root; Windows asks
/// the user to confirm) and PHP's bundle trust it.
pub fn ensure_trusted_ca(layout: &Layout, tld: &str, r: &mut dyn Report) -> io::Result<Ca> {
    let authority = Ca::load_or_create(layout, tld)?;
    if !ca::is_trusted(authority.der())? {
        r.info(
            "Windows will ask you to trust \"PHPev Local Development CA\" (it can only sign local development names)",
        );
        ca::trust(authority.der())?;
        r.info("CA trusted (CurrentUser\\Root)");
    }
    ca::update_php_bundle(layout, Some(authority.pem()))?;
    Ok(authority)
}

/// Turns HTTPS on or off for a site. Turning it on trusts the CA first.
pub fn set_site_secure(layout: &Layout, name: &str, secure: bool, r: &mut dyn Report) -> io::Result<sites::Site> {
    let site = find_site(layout, name)?;
    if secure {
        let tld = Config::load(&layout.config_file())?.tld;
        ensure_trusted_ca(layout, &tld, r)?;
    }
    config::update(&layout.config_file(), |c| {
        match c.sites.iter_mut().find(|s| s.name == name) {
            Some(s) => s.secure = secure,
            None if secure => c.sites.push(Site {
                name: site.name.clone(),
                path: site.path.clone(),
                php: None,
                secure: true,
                defender_excluded: false,
            }),
            None => {}
        }
    })?;
    Ok(site)
}

/// Absolute folder of a site (for "open folder/terminal" actions).
pub fn site_dir(layout: &Layout, name: &str) -> io::Result<PathBuf> {
    Ok(find_site(layout, name)?.path)
}

// ---------------------------------------------------------------- hosts file

/// True when sites need hosts entries (any TLD other than `localhost`).
pub fn needs_hosts(config: &Config) -> bool {
    config.tld != "localhost"
}

/// Rewrites the PHPev block of the hosts file through UAC: every site when
/// `include`, nothing (block removed) otherwise. Returns how many hosts.
pub fn write_hosts(config: &Config, include: bool) -> io::Result<usize> {
    let names: Vec<String> = if include {
        sites::all(config).iter().map(|s| s.host(&config.tld)).collect()
    } else {
        Vec::new()
    };
    if let Some(bad) = names.iter().find(|h| !hosts::is_valid_host(h)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid hostname {bad} (check `tld`)"),
        ));
    }
    let mut args = vec![elevate::CMD_HOSTS_SYNC];
    args.extend(names.iter().map(String::as_str));
    let code = elevate::run_elevated(&elevate::helper_path()?, &args)?;
    if code != 0 {
        return Err(io::Error::other(format!(
            "{} failed with exit code {code}",
            elevate::HELPER_EXE
        )));
    }
    Ok(names.len())
}
