//! Fixing what the site page's checks report. Blocking; call through
//! `spawn_blocking` or a job.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use phpev_core::config::{self, Config};
use phpev_core::layout::Layout;
use phpev_core::net::Http;
use phpev_core::ops::Report;
use phpev_core::shim::{self, Resolved};
use phpev_core::{composer, elevate, php, process, project};

/// The site's folder and the PHP its requests and commands run on.
pub fn site(layout: &Layout, name: &str) -> io::Result<(PathBuf, Resolved)> {
    let dir = phpev_core::ops::find_site(layout, name)?.path;
    let resolved = shim::resolve(layout.root(), &dir).map_err(|e| io::Error::other(e.to_string()))?;
    Ok((dir, resolved))
}

/// `php …` in `dir` with the site's PHP, as the `php` shim would run it.
fn php_in(resolved: &Resolved, dir: &Path) -> io::Result<Command> {
    let mut cmd = Command::new(resolved.dir.join("php.exe"));
    cmd.current_dir(dir).env("PHPRC", php::php_path(&resolved.dir)?);
    Ok(cmd)
}

fn run(mut cmd: Command, what: &str, rep: &mut dyn Report, cancel: &AtomicBool) -> io::Result<()> {
    let status = process::run_logged(&mut cmd, &mut |line| rep.info(line), cancel)?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("{what} failed (see the details)")))
    }
}

pub fn composer_install(
    layout: &Layout,
    dir: &Path,
    resolved: &Resolved,
    rep: &mut dyn Report,
    cancel: &AtomicBool,
) -> io::Result<String> {
    let dir_of = layout.composer_dir();
    let pinned = resolved.composer.as_deref();
    let default = || -> io::Result<Option<String>> { Ok(Config::load(&layout.config_file())?.composer) };
    let phar = match composer::pick(&dir_of, &resolved.version, pinned, default()?.as_deref()) {
        Ok(p) => p.phar,
        Err(e) => {
            // Only a missing Composer can be fixed here; one too new for the PHP is the user's pick.
            let installed = composer::installed(&dir_of);
            let has = |w: &str| phpev_core::version::best_match(w, installed.iter().map(String::as_str)).is_some();
            let wanted = match (pinned, default()?) {
                (Some(w), _) if !has(w) => Some(w.to_owned()),
                (None, Some(w)) if !has(&w) => Some(w),
                (None, None) if installed.is_empty() => None,
                _ => return Err(io::Error::other(e)),
            };
            rep.info("installing Composer first");
            phpev_core::ops::install_composer(&Http::new()?, layout, wanted.as_deref(), rep)?;
            composer::pick(&dir_of, &resolved.version, pinned, default()?.as_deref())
                .map_err(io::Error::other)?
                .phar
        }
    };
    rep.info(&format!("composer install (PHP {})", resolved.version));
    let mut cmd = php_in(resolved, dir)?;
    cmd.arg(&phar)
        .args(["install", "--no-interaction", "--no-ansi", "--no-progress"]);
    run(cmd, "composer install", rep, cancel)?;
    Ok("Dependencies installed".into())
}

/// `php artisan key:generate`, after creating `.env` from `.env.example` with `create_env`.
pub fn app_key(
    dir: &Path,
    resolved: &Resolved,
    create_env: bool,
    rep: &mut dyn Report,
    cancel: &AtomicBool,
) -> io::Result<String> {
    if create_env && !dir.join(".env").exists() {
        std::fs::copy(dir.join(".env.example"), dir.join(".env"))?;
        rep.info("copied .env.example to .env");
    }
    rep.info("php artisan key:generate");
    let mut cmd = php_in(resolved, dir)?;
    cmd.args(["artisan", "key:generate", "--force", "--no-ansi", "--no-interaction"]);
    run(cmd, "php artisan key:generate", rep, cancel)?;
    Ok("Application key set".into())
}

/// Turns on the required extensions the PHP build includes. Returns those
/// turned on and those it does not include (PECL extensions).
pub fn enable_extensions(layout: &Layout, dir: &Path, version: &str) -> io::Result<(Vec<String>, Vec<String>)> {
    let wanted = project::required_extensions(
        project::read_json(&dir.join("composer.json")).as_ref(),
        project::read_json(&dir.join("composer.lock")).as_ref(),
    );
    let loaded: Vec<String> = php::modules(layout, version)?
        .iter()
        .map(|m| project::normalize_module(m))
        .collect();
    let (mut enabled, mut unavailable) = (Vec::new(), Vec::new());
    for ext in wanted
        .iter()
        .filter(|e| !loaded.contains(&project::normalize_module(e)))
    {
        if php::has_extension_dll(layout, version, ext) {
            php::enable_extension(layout, version, ext)?;
            enabled.push(ext.clone());
        } else {
            unavailable.push(ext.clone());
        }
    }
    if !enabled.is_empty() {
        // Fails loudly if a DLL just turned on cannot load.
        php::modules(layout, version)?;
    }
    Ok((enabled, unavailable))
}

/// Adds a Defender exclusion for `dir` (Windows asks for permission) and remembers it.
pub fn defender(layout: &Layout, dir: &Path) -> io::Result<()> {
    let dir_text = dir.to_string_lossy();
    match elevate::run_elevated(&elevate::helper_path()?, &[elevate::CMD_DEFENDER_EXCLUDE, &dir_text])? {
        0 => {
            config::update(&layout.config_file(), |c: &mut Config| c.mark_defender_excluded(dir))?;
            Ok(())
        }
        code => Err(io::Error::other(format!(
            "Windows did not add the exclusion (exit code {code})"
        ))),
    }
}
