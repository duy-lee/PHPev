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
use crate::{clock, composer, elevate, hosts, mariadb, php, sites, version};

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
    ensure_cacert(http, layout, r)?;
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
    ensure_composer_lts(http, layout, &release.version, r);
    default_php_if_none(layout, &release.line, r)?;
    Ok(release.version.clone())
}

/// Adds the PHP build in `src` (for example an old version from another
/// tool): checks it, copies it into the PHPev home and writes a php.ini for
/// it; becomes the default if none is set. Returns its version.
pub fn add_php(http: &Http, layout: &Layout, src: &Path, r: &mut dyn Report) -> io::Result<String> {
    layout.create_dirs()?;
    let src = std::path::absolute(src)?;
    if !src.is_dir() {
        return Err(not_found(format!("{} is not a folder", src.display())));
    }
    if src.starts_with(layout.php_dir()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is already part of PHPev", src.display()),
        ));
    }
    let build = match php::inspect_folder(&src) {
        // php.exe could not start: usually a missing or older VC++ runtime.
        Err(e) if e.kind() == io::ErrorKind::Other && !php::vc_runtime_ok("vs17") => {
            r.info("PHP did not start; installing the Visual C++ runtime (Windows will ask for permission)");
            php::install_vc_runtime(http, layout, &mut |d, t| r.progress("vc_redist.x64.exe", d, t))?;
            php::inspect_folder(&src)?
        }
        other => other?,
    };
    let kind = if build.thread_safe { "TS" } else { "NTS" };
    r.info(&format!("PHP {} ({kind} x64) in {}", build.version, src.display()));
    if build.thread_safe {
        r.info("note: a thread-safe (TS) build works, but NTS builds are faster with PHPev");
    }

    let files = php::copy_folder(layout, &src, &build.version)?;
    let dir = php::version_dir(layout, &build.version);
    r.info(&format!("copied {files} files to {}", dir.display()));
    if dir.join("php.ini.orig").is_file() {
        r.info("kept its php.ini as php.ini.orig");
    }
    ensure_cacert(http, layout, r)?;
    if php::write_php_ini(layout, &build.version)? {
        r.info("wrote php.ini");
    }
    // A build from elsewhere may lack extensions PHPev expects; keep it anyway.
    match php::verify(layout, &build.version) {
        Ok(modules) => r.info(&format!("verified: {} extensions loaded", modules.len())),
        Err(e) => r.info(&format!("warning: {e}")),
    }
    let line = build.version.splitn(3, '.').take(2).collect::<Vec<_>>().join(".");
    ensure_composer_lts(http, layout, &build.version, r);
    default_php_if_none(layout, &line, r)?;
    Ok(build.version)
}

/// Downloads the CA bundle PHP uses if missing; a fresh bundle must carry the
/// PHPev CA again when HTTPS is in use.
fn ensure_cacert(http: &Http, layout: &Layout, r: &mut dyn Report) -> io::Result<()> {
    if php::ensure_cacert(http, layout, &mut |d, t| r.progress("cacert.pem", d, t))? && ca::exists(layout) {
        let tld = Config::load(&layout.config_file())?.tld;
        let authority = Ca::load_or_create(layout, &tld)?;
        ca::update_php_bundle(layout, Some(authority.pem()))?;
    }
    Ok(())
}

/// Composer 2.3+ does not run on PHP before 7.2.5: when Composer is in use,
/// fetch the 2.2 LTS phar the `composer` shim runs with such a PHP.
/// A failed download is only a warning: `phpev composer install` retries it.
fn ensure_composer_lts(http: &Http, layout: &Layout, version: &str, r: &mut dyn Report) {
    if composer::needs_lts(version) && composer::is_installed(layout) && !composer::lts_path(layout).is_file() {
        r.info("PHP before 7.2.5 needs Composer 2.2 LTS; installing it");
        if let Err(e) = composer::install_lts(http, layout, &mut |d, t| r.progress(composer::LTS_PHAR, d, t)) {
            r.info(&format!(
                "warning: Composer 2.2 LTS: {e} (run `phpev composer install` later)"
            ));
        }
    }
}

fn default_php_if_none(layout: &Layout, line: &str, r: &mut dyn Report) -> io::Result<()> {
    let became_default = config::update(&layout.config_file(), |c| {
        c.php.is_none().then(|| c.php = Some(line.to_owned())).is_some()
    })?;
    if became_default {
        r.info(&format!("set as default PHP ({line})"));
    }
    Ok(())
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

/// Linked sites pinned to a PHP that only `version` provides: they lose their
/// PHP when it is removed.
pub fn sites_pinned_to(layout: &Layout, version: &str) -> io::Result<Vec<String>> {
    let installed = php::installed(layout);
    let rest: Vec<&str> = installed.iter().map(String::as_str).filter(|v| *v != version).collect();
    let config = Config::load(&layout.config_file())?;
    Ok(config
        .sites
        .iter()
        .filter(|s| {
            s.php.as_deref().is_some_and(|w| {
                version::best_match(w, installed.iter().map(String::as_str)) == Some(version)
                    && version::best_match(w, rest.iter().copied()).is_none()
            })
        })
        .map(|s| s.name.clone())
        .collect())
}

/// Checks that PHP `version` can be removed (see [`remove_php`]).
pub fn check_remove_php(layout: &Layout, version: &str) -> io::Result<()> {
    let installed = php::installed(layout);
    if !installed.iter().any(|v| v == version) {
        return Err(not_found(format!(
            "PHP {version} is not installed (give the full version, as `phpev php list` shows it)"
        )));
    }
    let rest: Vec<&str> = installed.iter().map(String::as_str).filter(|v| *v != version).collect();
    let config = Config::load(&layout.config_file())?;
    if let Some(wanted) = config.php.as_deref() {
        let is_default = version::best_match(wanted, installed.iter().map(String::as_str)) == Some(version);
        if is_default && !rest.is_empty() && version::best_match(wanted, rest.iter().copied()).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "PHP {version} is the default; make another version the default first (phpev php use <version>)"
                ),
            ));
        }
    }
    Ok(())
}

/// Removes an installed PHP version (exact, e.g. `7.1.33`). Refused while it
/// is the default, unless it is the last one left. Its workers must be stopped
/// first. Sites pinned to it keep the pin; see [`sites_pinned_to`].
pub fn remove_php(layout: &Layout, version: &str, r: &mut dyn Report) -> io::Result<()> {
    check_remove_php(layout, version)?;
    let rest = php::installed(layout).into_iter().filter(|v| v != version).count();
    let config = Config::load(&layout.config_file())?;
    remove_version_dir(&php::version_dir(layout, version), &format!("PHP {version}"), r)?;
    r.info(&format!("removed PHP {version}"));

    if rest == 0 && config.php.is_some() {
        config::update(&layout.config_file(), |c| c.php = None)?;
        r.info("no PHP left; cleared the default");
    }
    if composer::lts_path(layout).is_file() && !composer::lts_wanted(layout) {
        std::fs::remove_file(composer::lts_path(layout))?;
        r.info("removed Composer 2.2 LTS (no PHP before 7.2.5 left)");
    }
    Ok(())
}

/// Deletes an installed version folder, unless a program still runs from it
/// (Windows would delete the other files and leave a broken version).
fn remove_version_dir(dir: &Path, what: &str, r: &mut dyn Report) -> io::Result<()> {
    let running = crate::procinfo::running_from(dir);
    if !running.is_empty() {
        return Err(io::Error::other(format!(
            "{what} is in use by {}; close these programs and try again",
            running.join(", ")
        )));
    }
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parent = dir.parent().unwrap_or(dir);
    // Leftovers of earlier removals that could not finish.
    for old in std::fs::read_dir(parent).into_iter().flatten().flatten() {
        if old.file_name().to_string_lossy().starts_with(".removing-") {
            let _ = std::fs::remove_dir_all(old.path());
        }
    }
    // Renamed first, so a deletion that stops halfway never leaves a version
    // that looks installed.
    let trash = parent.join(format!(".removing-{name}-{}", std::process::id()));
    std::fs::rename(dir, &trash)?;
    if let Err(e) = std::fs::remove_dir_all(&trash) {
        r.info(&format!(
            "warning: could not delete everything in {}: {e}",
            trash.display()
        ));
    }
    Ok(())
}

/// Total size of the files under `path`.
pub fn dir_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    std::fs::read_dir(path)
        .map(|entries| entries.flatten().map(|e| dir_size(&e.path())).sum())
        .unwrap_or(0)
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

/// What [`remove_mariadb`] would remove.
pub struct MariaDbRemoval {
    pub version: String,
    pub major: String,
    /// No other release of this major stays installed, so its databases
    /// (`data_dir`) are no longer used.
    pub last_of_major: bool,
    pub data_dir: PathBuf,
}

/// Checks that MariaDB `wanted` (a version as `phpev db list` shows it, or its
/// major) can be removed: installed, and not the version in use.
pub fn plan_remove_mariadb(layout: &Layout, wanted: &str) -> io::Result<MariaDbRemoval> {
    let installed = mariadb::installed(layout);
    let version = version::best_match(wanted, installed.iter().map(String::as_str))
        .ok_or_else(|| not_found(format!("MariaDB {wanted} is not installed")))?
        .to_owned();
    let major = mariadb::major_of(&version);
    let config = Config::load(&layout.config_file())?;
    if config.mariadb.as_deref() == Some(major.as_str())
        && mariadb::installed_for_major(layout, &major).as_deref() == Some(version.as_str())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("MariaDB {version} is the active version; switch to another one first (phpev db use <version>)"),
        ));
    }
    let last_of_major = !installed.iter().any(|v| *v != version && mariadb::major_of(v) == major);
    Ok(MariaDbRemoval {
        data_dir: mariadb::data_dir(layout, &major),
        version,
        major,
        last_of_major,
    })
}

/// Removes an installed MariaDB, and with `delete_data` the databases of its
/// major when no other release of that major is left. Returns its version.
pub fn remove_mariadb(layout: &Layout, wanted: &str, delete_data: bool, r: &mut dyn Report) -> io::Result<String> {
    let plan = plan_remove_mariadb(layout, wanted)?;
    remove_version_dir(
        &mariadb::version_dir(layout, &plan.version),
        &format!("MariaDB {}", plan.version),
        r,
    )?;
    r.info(&format!("removed MariaDB {}", plan.version));
    if plan.last_of_major && plan.data_dir.is_dir() {
        if delete_data {
            std::fs::remove_dir_all(&plan.data_dir)?;
            r.info(&format!("deleted the databases in {}", plan.data_dir.display()));
        } else {
            r.info(&format!("kept the databases in {}", plan.data_dir.display()));
        }
    }
    Ok(plan.version)
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

// ---------------------------------------------------------------- parked folders and TLD

fn same_dir(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .trim_end_matches('\\')
        .eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches('\\'))
}

/// Parks `dir` (absolute, existing): each sub-folder becomes a site.
/// Returns false if it was already parked.
pub fn park(layout: &Layout, dir: &Path) -> io::Result<bool> {
    if !dir.is_absolute() || !dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not an existing folder", dir.display()),
        ));
    }
    config::update(&layout.config_file(), |c| {
        let new = !c.parked.iter().any(|p| same_dir(p, dir));
        if new {
            c.parked.push(dir.to_owned());
        }
        new
    })
}

/// Stops parking `dir`. Returns false if it was not parked.
pub fn unpark(layout: &Layout, dir: &Path) -> io::Result<bool> {
    config::update(&layout.config_file(), |c| {
        let before = c.parked.len();
        c.parked.retain(|p| !same_dir(p, dir));
        before != c.parked.len()
    })
}

/// `tld` without a leading dot, lower case, if it is a single DNS label.
pub fn normalize_tld(tld: &str) -> io::Result<String> {
    let tld = tld.trim().trim_start_matches('.').to_ascii_lowercase();
    if tld.is_empty()
        || tld.len() > 63
        || tld.starts_with('-')
        || !tld
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a TLD is one label: a-z, 0-9, -",
        ));
    }
    Ok(tld)
}

/// What [`set_tld`] changed.
pub struct TldChange {
    pub old: String,
    pub new: String,
    /// The hosts file could not be updated (the change itself is saved).
    pub hosts_error: Option<String>,
    /// HTTPS is in use: the daemon must restart to serve the new names.
    pub https_in_use: bool,
}

/// Changes the TLD. Leaving `localhost` writes the sites into the hosts file
/// and going back removes them (both through UAC); a TLD the CA does not
/// cover yet changes its name constraints, hence a new trust prompt.
pub fn set_tld(layout: &Layout, wanted: &str, r: &mut dyn Report) -> io::Result<TldChange> {
    let new = normalize_tld(wanted)?;
    let old = config::update(&layout.config_file(), |c| std::mem::replace(&mut c.tld, new.clone()))?;
    let config = Config::load(&layout.config_file())?;
    let mut change = TldChange {
        https_in_use: ca::exists(layout) && config.sites.iter().any(|s| s.secure),
        old,
        new,
        hosts_error: None,
    };
    if change.old == change.new {
        return Ok(change);
    }
    if needs_hosts(&config) || change.old != "localhost" {
        r.info("Windows will ask for permission to update the hosts file");
        if let Err(e) = write_hosts(&config, needs_hosts(&config)) {
            change.hosts_error = Some(e.to_string());
        }
    }
    if change.https_in_use {
        ensure_trusted_ca(layout, &change.new, r)?;
    }
    Ok(change)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::HomeSource;

    fn home(name: &str, php: &[&str], mariadb: &[&str], config: Config) -> Layout {
        let root = std::env::temp_dir().join(format!("phpev-ops-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let layout = Layout::at(root, HomeSource::ProcessEnv);
        layout.create_dirs().unwrap();
        for v in php {
            let d = php::version_dir(&layout, v);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("php.exe"), "").unwrap();
        }
        for v in mariadb {
            let d = mariadb::version_dir(&layout, v).join("bin");
            std::fs::create_dir_all(&d).unwrap();
            for exe in mariadb::KEEP_EXES {
                std::fs::write(d.join(exe), "").unwrap();
            }
            std::fs::create_dir_all(mariadb::data_dir(&layout, &mariadb::major_of(v))).unwrap();
        }
        config.save(&layout.config_file()).unwrap();
        layout
    }

    fn site(name: &str, php: &str) -> Site {
        Site {
            name: name.into(),
            path: PathBuf::from(format!(r"C:\sites\{name}")),
            php: Some(php.into()),
            secure: false,
            defender_excluded: false,
        }
    }

    #[test]
    fn the_default_php_is_kept_until_another_one_is_the_default() {
        let config = Config {
            php: Some("7.1".into()),
            sites: vec![site("old", "7.1"), site("new", "8.4")],
            ..Config::default()
        };
        let layout = home("php-default", &["8.4.27", "7.1.33"], &[], config);
        std::fs::write(composer::lts_path(&layout), "").unwrap();

        let refused = remove_php(&layout, "7.1.33", &mut Quiet).unwrap_err();
        assert!(refused.to_string().contains("is the default"), "{refused}");
        assert_eq!(sites_pinned_to(&layout, "7.1.33").unwrap(), ["old"]);
        assert!(remove_php(&layout, "7.1", &mut Quiet).is_err(), "only full versions");

        set_default_php(&layout, "8.4").unwrap();
        remove_php(&layout, "7.1.33", &mut Quiet).unwrap();
        assert_eq!(php::installed(&layout), ["8.4.27"]);
        assert!(!composer::lts_path(&layout).exists(), "no PHP before 7.2.5 is left");

        // The last PHP may go; the default then points nowhere and is cleared.
        remove_php(&layout, "8.4.27", &mut Quiet).unwrap();
        assert!(php::installed(&layout).is_empty());
        assert_eq!(Config::load(&layout.config_file()).unwrap().php, None);
        let _ = std::fs::remove_dir_all(layout.root());
    }

    #[test]
    fn mariadb_in_use_stays_and_databases_go_only_when_asked() {
        let config = Config {
            mariadb: Some("11.4".into()),
            ..Config::default()
        };
        let layout = home("mariadb", &[], &["11.4.13", "10.11.14", "10.6.23"], config);

        let refused = remove_mariadb(&layout, "11.4", false, &mut Quiet).unwrap_err();
        assert!(refused.to_string().contains("active"), "{refused}");

        let plan = plan_remove_mariadb(&layout, "10.11").unwrap();
        assert_eq!(plan.version, "10.11.14");
        assert!(plan.last_of_major);
        remove_mariadb(&layout, "10.11", false, &mut Quiet).unwrap();
        assert!(mariadb::data_dir(&layout, "10.11").is_dir(), "kept by default");

        remove_mariadb(&layout, "10.6.23", true, &mut Quiet).unwrap();
        assert!(!mariadb::data_dir(&layout, "10.6").exists());
        assert_eq!(mariadb::installed(&layout), ["11.4.13"]);
        let _ = std::fs::remove_dir_all(layout.root());
    }
}
