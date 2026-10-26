//! `phpev import [dir]`: make an existing PHP/Laravel project run.
//!
//! Every step is idempotent. Steps with side effects ask first (y/N);
//! `--yes` answers yes to all of them except migrations, which need
//! `--migrate`. Decisions (2026-10-23): the PHP version is pinned in PHPev's
//! config, never written into the repo; an existing `.env` is never edited,
//! only checked; MariaDB is used as root without a password.

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use phpev_core::config::{self, Config, Site};
use phpev_core::constraint::Constraint;
use phpev_core::ipc::Request;
use phpev_core::layout::Layout;
use phpev_core::net::Http;
use phpev_core::{composer, elevate, envfile, mariadb, php, project, shim, sites, version};

use crate::{daemon, runtimes};

pub struct Options {
    pub dir: Option<PathBuf>,
    pub name: Option<String>,
    pub yes: bool,
    pub migrate: bool,
    /// Add a Microsoft Defender exclusion for the project without asking.
    pub defender_exclude: bool,
}

/// Default MariaDB major when a project needs one and none is installed.
const DEFAULT_MARIADB: &str = "11.4";

pub fn import(opts: Options) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    layout.create_dirs()?;
    let dir = crate::sites::absolute(opts.dir.clone())?;
    let ask = Asker { yes: opts.yes };

    let composer_json = project::read_json(&dir.join("composer.json"));
    let lock_json = project::read_json(&dir.join("composer.lock"));
    let laravel = project::is_laravel(&dir, composer_json.as_ref());
    println!("Project: {}", dir.display());
    if laravel {
        let v = lock_json
            .as_ref()
            .and_then(|l| project::package_version(l, "laravel/framework"))
            .unwrap_or_else(|| "?".into());
        println!("  Laravel {v}");
    } else if composer_json.is_some() {
        println!("  PHP project (composer.json, no Laravel)");
    } else {
        println!("  plain PHP folder (no composer.json)");
    }

    // ---- 1. PHP version
    step("PHP");
    let (php_version, pin) = choose_php(&layout, &dir, composer_json.as_ref(), &ask)?;
    let Some(php_version) = php_version else {
        println!("Stopped: no suitable PHP is installed.");
        return Ok(ExitCode::FAILURE);
    };

    // ---- 2. Extensions
    let mut todo: Vec<String> = Vec::new();
    step("Extensions");
    check_extensions(
        &layout,
        &php_version,
        composer_json.as_ref(),
        lock_json.as_ref(),
        &ask,
        &mut todo,
    )?;

    // ---- 3. Defender exclusion (before composer install, which benefits most)
    step("Defender");
    let excluded = defender_exclusion(&layout, &dir, &opts, &ask)?;

    // ---- 4. Composer dependencies
    if composer_json.is_some() {
        step("Composer");
        let phar = composer::phar_for(&layout.composer_dir(), &php_version);
        if !phar.is_file() && ask.confirm("Composer is not installed. Install it?") {
            runtimes::composer_install(false)?;
        }
        if dir.join(r"vendor\autoload.php").is_file() {
            println!("  vendor/ present");
        } else if !phar.is_file() {
            todo.push("install Composer, then run `composer install`".into());
        } else if ask.confirm("vendor/ is missing. Run `composer install` now?") {
            let ok = run_php(
                &layout,
                &php_version,
                &dir,
                &[phar.as_os_str(), "install".as_ref(), "--no-interaction".as_ref()],
            )?;
            if !ok {
                println!("  composer install failed (see above)");
                todo.push("fix the error above and run `composer install`".into());
            }
        } else {
            todo.push("run `composer install`".into());
        }
    }

    // ---- 4. Site / domain
    step("Site");
    let name = site_name(&dir, opts.name.as_deref())?;
    link_site(&layout, &dir, &name, pin.clone(), excluded)?;
    crate::sites::auto_hosts(&layout);
    let config = Config::load(&layout.config_file())?;
    let port_suffix = if config.ports.http == 80 {
        String::new()
    } else {
        format!(":{}", config.ports.http)
    };
    let url = format!("http://{name}.{}{port_suffix}", config.tld);
    match &pin {
        Some(p) => println!("  {url}  (PHP {p} pinned in PHPev config)"),
        None => println!("  {url}  (PHP from the project's .php-version)"),
    }

    // ---- 5–7. Laravel: .env, database, migrations
    let mut db_summary = None;
    if laravel {
        step(".env");
        let env_text = prepare_env(&layout, &dir, &php_version, &url, &config, &ask, &mut todo)?;
        if let Some(env_text) = env_text {
            step("Database");
            db_summary = prepare_database(&layout, &dir, &env_text, &config, &ask, &mut todo)?;
            if opts.migrate {
                println!("  running `php artisan migrate --force`");
                if !run_php(
                    &layout,
                    &php_version,
                    &dir,
                    &["artisan".as_ref(), "migrate".as_ref(), "--force".as_ref()],
                )? {
                    todo.push("fix the migration error above and run `php artisan migrate`".into());
                }
            } else {
                match migrations_pending(&layout, &php_version, &dir) {
                    Some(false) => println!("  migrations are up to date"),
                    Some(true) => todo.push("run `php artisan migrate` (or re-run import with --migrate)".into()),
                    None => todo.push("check `php artisan migrate:status` (it could not be read)".into()),
                }
            }
        }
    }

    // ---- 8. Front-end assets (Node is out of scope for phase 1)
    if dir.join("package.json").is_file()
        && !dir.join(r"public\build\manifest.json").is_file()
        && !dir.join(r"public\hot").exists()
    {
        todo.push("build front-end assets: `npm install && npm run build` (or `npm run dev`)".into());
    }

    println!("\nReady: {url}");
    println!(
        "  PHP {php_version}{}",
        db_summary.map(|d| format!(" · {d}")).unwrap_or_default()
    );
    if !todo.is_empty() {
        println!("Still to do:");
        for t in &todo {
            println!("  - {t}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn step(title: &str) {
    println!("\n[{title}]");
}

// ---------------------------------------------------------------- PHP

/// Returns (installed version to use, version pin for the config).
/// The pin is `None` when the project's own `.php-version` decides.
fn choose_php(
    layout: &Layout,
    dir: &Path,
    composer_json: Option<&serde_json::Value>,
    ask: &Asker,
) -> io::Result<(Option<String>, Option<String>)> {
    if let Some((file, wanted)) = shim::find_version_file(dir) {
        println!("  {} asks for PHP {wanted}", file.display());
        let installed = php::installed(layout);
        if let Some(v) = version::best_match(&wanted, installed.iter().map(String::as_str)) {
            println!("  using PHP {v}");
            return Ok((Some(v.to_owned()), None));
        }
        if ask.confirm(&format!("PHP {wanted} is not installed. Install it?")) {
            runtimes::php_install(&wanted)?;
            let installed = php::installed(layout);
            return Ok((
                version::best_match(&wanted, installed.iter().map(String::as_str)).map(str::to_owned),
                None,
            ));
        }
        return Ok((None, None));
    }

    let require = composer_json.and_then(|c| c.pointer("/require/php")?.as_str().map(str::to_owned));
    let platform = composer_json.and_then(|c| c.pointer("/config/platform/php")?.as_str().map(str::to_owned));
    let constraint = match &require {
        Some(text) => match Constraint::parse(text) {
            Some(c) => {
                println!("  composer.json requires php {text}");
                Some(c)
            }
            None => {
                println!("  cannot understand php constraint `{text}`; using any version");
                None
            }
        },
        None => {
            println!("  composer.json does not require a PHP version");
            None
        }
    };
    let fits = |v: &str| constraint.as_ref().is_none_or(|c| c.matches(v));
    // config.platform.php: Composer resolved dependencies for that minor version, so prefer it.
    let platform_line = platform
        .as_deref()
        .map(|p| p.split('.').take(2).collect::<Vec<_>>().join("."));
    if let Some(p) = &platform {
        println!(
            "  config.platform.php = {p} (preferring PHP {})",
            platform_line.as_deref().unwrap_or(p)
        );
    }
    let prefer = |list: &[String]| -> Option<String> {
        let ok: Vec<&String> = list.iter().filter(|v| fits(v)).collect();
        platform_line
            .as_ref()
            .and_then(|line| ok.iter().find(|v| version::matches(line, v)))
            .or_else(|| ok.first())
            .map(|v| (*v).clone())
    };

    let installed = php::installed(layout); // newest first
    if let Some(v) = prefer(&installed) {
        println!("  using installed PHP {v}");
        return Ok((Some(v.clone()), Some(line_of(&v))));
    }

    let releases = php::available(&Http::new()?)?;
    let versions: Vec<String> = releases.iter().map(|r| r.version.clone()).collect();
    let Some(candidate) = prefer(&versions) else {
        println!("  no PHP release on windows.php.net satisfies the constraint");
        return Ok((None, None));
    };
    if !ask.confirm(&format!("No installed PHP fits. Install PHP {candidate}?")) {
        return Ok((None, None));
    }
    runtimes::php_install(&candidate)?;
    Ok((Some(candidate.clone()), Some(line_of(&candidate))))
}

/// `8.4.27` → `8.4`: pin the line so patch updates are picked up.
fn line_of(v: &str) -> String {
    v.split('.').take(2).collect::<Vec<_>>().join(".")
}

// ---------------------------------------------------------------- extensions

fn check_extensions(
    layout: &Layout,
    version: &str,
    composer_json: Option<&serde_json::Value>,
    lock_json: Option<&serde_json::Value>,
    ask: &Asker,
    todo: &mut Vec<String>,
) -> io::Result<()> {
    let wanted = project::required_extensions(composer_json, lock_json);
    if wanted.is_empty() {
        println!("  no ext-* requirements");
        return Ok(());
    }

    let loaded: Vec<String> = php::modules(layout, version)?
        .iter()
        .map(|m| project::normalize_module(m))
        .collect();
    let mut missing_pecl = Vec::new();
    let mut enabled_any = false;
    for ext in &wanted {
        let name = project::normalize_module(ext);
        if loaded.contains(&name) {
            continue;
        }
        if php::has_extension_dll(layout, version, ext) {
            let q = format!("Extension `{ext}` is required but off. Enable it in PHP {version}'s php.ini (applies to every project on {version})?");
            if ask.confirm(&q) {
                php::enable_extension(layout, version, ext)?;
                enabled_any = true;
                println!("  enabled {ext}");
            } else {
                todo.push(format!("enable extension `{ext}` (phpev php ini {version})"));
            }
        } else {
            missing_pecl.push(ext.clone());
        }
    }
    if enabled_any {
        // Fails loudly if a just-enabled DLL cannot load.
        php::modules(layout, version)?;
        // Running php-cgi workers loaded the old php.ini.
        if phpev_core::ipc::is_running() {
            if let Ok(msg) = daemon::request(&Request::PhpRestart {
                version: version.to_owned(),
            }) {
                println!("  {msg}");
            }
        }
    }
    for ext in &missing_pecl {
        println!("  warning: `{ext}` is not part of the official PHP build (PECL); install it manually");
        todo.push(format!("install the PECL extension `{ext}` for PHP {version}"));
    }
    if missing_pecl.is_empty() && !enabled_any {
        println!("  all {} required extensions are loaded", wanted.len());
    }
    Ok(())
}

// ---------------------------------------------------------------- site

fn site_name(dir: &Path, explicit: Option<&str>) -> io::Result<String> {
    let folder = dir
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match explicit {
        Some(n) => sites::normalize_name(n).filter(|norm| norm == n),
        None => sites::normalize_name(&folder),
    };
    name.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid site name; pass --name <a-z0-9->"))
}

fn link_site(layout: &Layout, dir: &Path, name: &str, pin: Option<String>, excluded: bool) -> io::Result<()> {
    let clash = config::update(&layout.config_file(), |c| {
        if let Some(other) = c.sites.iter().find(|s| s.name == name && !same_path(&s.path, dir)) {
            return Some(other.path.clone());
        }
        let previous = c.sites.iter().find(|s| s.name == name);
        let secure = previous.is_some_and(|s| s.secure);
        let defender_excluded = excluded || previous.is_some_and(|s| s.defender_excluded);
        c.sites.retain(|s| s.name != name);
        c.sites.push(Site {
            name: name.to_owned(),
            path: dir.to_owned(),
            php: pin.clone(),
            secure,
            defender_excluded,
        });
        None
    })?;
    match clash {
        Some(other) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "site `{name}` already points to {}; choose another with --name",
                other.display()
            ),
        )),
        None => Ok(()),
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

// ---------------------------------------------------------------- Defender

/// Offers a Defender exclusion for the project folder. Never implied by
/// `--yes` (decided 2026-10-23): it trades malware scanning for speed, so it
/// needs a typed "y" or `--defender-exclude`. Returns whether the folder is
/// excluded (now or by an earlier import).
fn defender_exclusion(layout: &Layout, dir: &Path, opts: &Options, ask: &Asker) -> io::Result<bool> {
    let config = Config::load(&layout.config_file())?;
    if config
        .sites
        .iter()
        .any(|s| s.defender_excluded && same_path(&s.path, dir))
    {
        println!("  already excluded by an earlier import");
        return Ok(true);
    }
    let question = "Exclude this folder from Microsoft Defender real-time scanning? Speeds up composer/npm \
                    install and artisan, but malware written into this folder (e.g. a compromised package) \
                    is not caught on write. Needs admin";
    if !ask.confirm_explicit(question, opts.defender_exclude, "--defender-exclude") {
        return Ok(false);
    }
    let dir_str = dir.to_string_lossy();
    match elevate::run_elevated(&elevate::helper_path()?, &[elevate::CMD_DEFENDER_EXCLUDE, &dir_str]) {
        Ok(0) => {
            println!("  excluded {}", dir.display());
            Ok(true)
        }
        Ok(code) => {
            println!("  could not add the exclusion (exit code {code}); continuing");
            Ok(false)
        }
        Err(e) => {
            println!("  could not add the exclusion ({e}); continuing");
            Ok(false)
        }
    }
}

// ---------------------------------------------------------------- .env

/// Creates `.env` from `.env.example` (filling PHPev values) or, if it exists,
/// only reports mismatches. Returns the resulting `.env` text, if any.
fn prepare_env(
    layout: &Layout,
    dir: &Path,
    php_version: &str,
    url: &str,
    config: &Config,
    ask: &Asker,
    todo: &mut Vec<String>,
) -> io::Result<Option<String>> {
    let env_path = dir.join(".env");
    if let Ok(text) = std::fs::read_to_string(&env_path) {
        println!("  .env exists — not modified");
        let mut notes = Vec::new();
        if envfile::get(&text, "APP_KEY").is_none_or(|k| k.is_empty()) {
            notes.push("APP_KEY is empty → run `php artisan key:generate`".to_owned());
        }
        if envfile::get(&text, "APP_URL").as_deref() != Some(url) {
            notes.push(format!("APP_URL is not {url}"));
        }
        if is_mysql(&text) {
            notes.extend(mysql_mismatches(&text, config.ports.mariadb));
        }
        for n in &notes {
            println!("  note: {n}");
        }
        return Ok(Some(text));
    }

    let example = dir.join(".env.example");
    let Ok(mut text) = std::fs::read_to_string(&example) else {
        println!("  no .env and no .env.example");
        todo.push("create .env".into());
        return Ok(None);
    };
    if !ask.confirm("Create .env from .env.example with PHPev's URL and database settings?") {
        todo.push("create .env (copy .env.example, then `php artisan key:generate`)".into());
        return Ok(None);
    }
    text = envfile::set(&text, "APP_URL", url);
    if is_mysql(&text) {
        text = envfile::set(&text, "DB_HOST", "127.0.0.1");
        text = envfile::set(&text, "DB_PORT", &config.ports.mariadb.to_string());
        text = envfile::set(&text, "DB_USERNAME", "root");
        text = envfile::set(&text, "DB_PASSWORD", "");
    }
    std::fs::write(&env_path, &text)?;
    println!("  created .env (APP_URL={url})");
    if run_php(
        layout,
        php_version,
        dir,
        &["artisan".as_ref(), "key:generate".as_ref(), "--force".as_ref()],
    )? {
        text = std::fs::read_to_string(&env_path)?;
    } else {
        todo.push("run `php artisan key:generate`".into());
    }
    Ok(Some(text))
}

fn is_mysql(env: &str) -> bool {
    matches!(envfile::get(env, "DB_CONNECTION").as_deref(), Some("mysql" | "mariadb"))
}

fn mysql_mismatches(env: &str, port: u16) -> Vec<String> {
    let mut out = Vec::new();
    let host = envfile::get(env, "DB_HOST").unwrap_or_else(|| "127.0.0.1".into());
    if !matches!(host.as_str(), "127.0.0.1" | "localhost") {
        out.push(format!("DB_HOST={host}; PHPev's MariaDB listens on 127.0.0.1"));
    }
    let env_port = envfile::get(env, "DB_PORT").unwrap_or_else(|| "3306".into());
    if env_port != port.to_string() {
        out.push(format!("DB_PORT={env_port}; PHPev's MariaDB uses {port}"));
    }
    let user = envfile::get(env, "DB_USERNAME").unwrap_or_else(|| "root".into());
    let pass = envfile::get(env, "DB_PASSWORD").unwrap_or_default();
    if user != "root" || !pass.is_empty() {
        out.push("PHPev's MariaDB uses DB_USERNAME=root with an empty DB_PASSWORD".into());
    }
    out
}

// ---------------------------------------------------------------- database

fn prepare_database(
    layout: &Layout,
    dir: &Path,
    env: &str,
    config: &Config,
    ask: &Asker,
    todo: &mut Vec<String>,
) -> io::Result<Option<String>> {
    let connection = envfile::get(env, "DB_CONNECTION").unwrap_or_else(|| "sqlite".into());
    match connection.as_str() {
        "sqlite" => {
            let configured = envfile::get(env, "DB_DATABASE").filter(|d| !d.is_empty());
            let file = match configured {
                Some(p) if Path::new(&p).is_absolute() => PathBuf::from(p),
                Some(p) => dir.join(p),
                None => dir.join(r"database\database.sqlite"),
            };
            if !file.exists() {
                if let Some(parent) = file.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::File::create(&file)?;
                println!("  created {}", file.display());
            } else {
                println!("  SQLite file present");
            }
            Ok(Some("SQLite".into()))
        }
        "mysql" | "mariadb" => {
            let db = envfile::get(env, "DB_DATABASE").unwrap_or_else(|| "laravel".into());
            let major = config.mariadb.clone().unwrap_or_else(|| DEFAULT_MARIADB.to_owned());
            if mariadb::installed_for_major(layout, &major).is_none() {
                if !ask.confirm(&format!(
                    "The project uses MySQL/MariaDB but MariaDB {major} is not installed. Install it?"
                )) {
                    todo.push(format!(
                        "install MariaDB (`phpev db install {major}`) and create database `{db}`"
                    ));
                    return Ok(None);
                }
                runtimes::db_install(&major)?;
            }
            let version = mariadb::installed_for_major(layout, &major)
                .ok_or_else(|| io::Error::other(format!("MariaDB {major} is still not installed")))?;
            println!("  starting MariaDB {version}…");
            daemon::ensure_running()?;
            println!("  {}", daemon::request(&Request::DbStart)?);

            if !mariadb::is_safe_db_name(&db) {
                println!("  warning: database name `{db}` contains unusual characters; create it yourself");
                todo.push(format!("create database `{db}`"));
            } else {
                let port = config.ports.mariadb;
                let sql =
                    format!("CREATE DATABASE IF NOT EXISTS `{db}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci");
                mariadb::exec_sql(layout, &version, port, &sql)?;
                println!("  database `{db}` ready on 127.0.0.1:{port}");
            }
            for n in mysql_mismatches(env, config.ports.mariadb) {
                println!("  note: {n}");
                todo.push(format!("update .env: {n}"));
            }
            Ok(Some(format!("MariaDB {version} / {db}")))
        }
        other => {
            println!("  warning: DB_CONNECTION={other} is not provided by PHPev (MariaDB and SQLite only)");
            todo.push(format!("set up a {other} server yourself"));
            Ok(None)
        }
    }
}

// ---------------------------------------------------------------- helpers

/// Asks Laravel whether migrations are pending. `None` if it cannot tell
/// (database unreachable, no migrations table yet counts as pending).
fn migrations_pending(layout: &Layout, version: &str, dir: &Path) -> Option<bool> {
    let vdir = php::version_dir(layout, version);
    let out = Command::new(vdir.join("php.exe"))
        .args(["artisan", "migrate:status", "--no-ansi"])
        .current_dir(dir)
        .env("PHPRC", php::php_path(&vdir).ok()?)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    if text.contains("Migration table not found") {
        return Some(true);
    }
    out.status.success().then(|| text.contains("Pending"))
}

/// Runs the chosen PHP in `dir` with its php.ini (same environment as the shim).
fn run_php(layout: &Layout, version: &str, dir: &Path, args: &[&std::ffi::OsStr]) -> io::Result<bool> {
    let vdir = php::version_dir(layout, version);
    let status = Command::new(vdir.join("php.exe"))
        .args(args)
        .current_dir(dir)
        .env("PHPRC", php::php_path(&vdir)?)
        .status()?;
    Ok(status.success())
}

struct Asker {
    yes: bool,
}

impl Asker {
    /// y/N prompt that `--yes` does NOT answer: only `flag` or a typed "y" agrees.
    fn confirm_explicit(&self, question: &str, flag: bool, flag_name: &str) -> bool {
        if flag {
            println!("  {question}: yes ({flag_name})");
            return true;
        }
        if !io::stdin().is_terminal() {
            println!("  {question}: skipped (pass {flag_name} to do it)");
            return false;
        }
        print!("  {question}. Proceed? [y/N] ");
        let _ = io::stdout().flush();
        let mut line = String::new();
        if io::stdin().lock().read_line(&mut line).is_err() {
            return false;
        }
        matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
    }

    /// y/N prompt. `--yes` answers yes; without a terminal the answer is no.
    fn confirm(&self, question: &str) -> bool {
        if self.yes {
            println!("  {question} yes (--yes)");
            return true;
        }
        if !io::stdin().is_terminal() {
            println!("  {question} skipped (no terminal; re-run with --yes)");
            return false;
        }
        print!("  {question} [y/N] ");
        let _ = io::stdout().flush();
        let mut line = String::new();
        if io::stdin().lock().read_line(&mut line).is_err() {
            return false;
        }
        matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_env_checks() {
        let ok = "DB_CONNECTION=mysql\nDB_HOST=127.0.0.1\nDB_PORT=3306\nDB_USERNAME=root\nDB_PASSWORD=\n";
        assert!(mysql_mismatches(ok, 3306).is_empty());
        let bad = "DB_CONNECTION=mysql\nDB_HOST=db\nDB_PORT=3307\nDB_USERNAME=laravel\nDB_PASSWORD=secret\n";
        assert_eq!(mysql_mismatches(bad, 3306).len(), 3);
    }
}
