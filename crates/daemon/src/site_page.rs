//! Data for the dashboard's site page: what the project is, how PHPev runs it
//! and whether it is ready. Blocking; call through `spawn_blocking`.

use std::collections::HashMap;
use std::io;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

use phpev_core::api::{SiteDatabase, SiteDb, SiteDetail, SiteInfo};
use phpev_core::config::Config;
use phpev_core::layout::Layout;
use phpev_core::shim::{self, Source};
use phpev_core::{ca, composer, ipc, mariadb, php, project, version};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn detail(
    layout: &Layout,
    site: SiteInfo,
    pools: &[ipc::Pool],
    db_server: Option<&ipc::Service>,
) -> io::Result<SiteDetail> {
    let config = Config::load(&layout.config_file())?;
    let dir = PathBuf::from(&site.path);
    let resolved = shim::resolve(layout.root(), &dir).ok();
    let php_version = resolved.as_ref().map(|r| r.version.clone());

    let mut php_lines: Vec<String> = php::installed(layout)
        .iter()
        .map(|v| v.split('.').take(2).collect::<Vec<_>>().join("."))
        .collect();
    php_lines.dedup();

    let pool = php_version
        .as_ref()
        .and_then(|v| pools.iter().find(|p| &p.version == v));
    let workers = pool.map_or(0, |p| p.workers.len());
    let workers_memory_kb = pool.map_or(0, |p| p.workers.iter().filter_map(|w| w.memory_kb).sum());

    let databases: Vec<SiteDb> = project::databases(&dir)
        .into_iter()
        .map(|settings| SiteDb {
            local: settings.is_local_mysql(config.ports.mariadb) && !settings.database.is_empty(),
            settings,
        })
        .collect();
    let existing = db_server.and_then(|server| mariadb::database_names(layout, &server.version, server.port).ok());
    let modules = php_version.as_ref().and_then(|v| php::modules(layout, v).ok());
    let checks = project::checks(
        &dir,
        &project::Facts {
            modules: modules.as_deref(),
            existing_databases: existing.as_deref(),
        },
        config.ports.mariadb,
    );

    let logs = crate::logs::sources(layout)?
        .into_iter()
        .filter(|l| {
            l.id.strip_prefix("site:")
                .is_some_and(|rest| rest.split(':').next() == Some(site.name.as_str()))
                || php_version.as_ref().is_some_and(|v| l.id == format!("php:{v}"))
        })
        .collect();

    Ok(SiteDetail {
        php_source: match resolved.as_ref().map(|r| &r.source) {
            Some(Source::Site(_)) => "pinned",
            Some(Source::VersionFile(_)) => "version-file",
            Some(Source::Default) => "default",
            _ => "newest",
        }
        .into(),
        php_lines,
        workers,
        workers_memory_kb,
        ca_trusted: ca::stored_der(layout).is_some_and(|der| ca::is_trusted(&der).unwrap_or(false)),
        composer: resolved
            .as_ref()
            .and_then(|r| composer_version(layout, &r.dir, &r.version)),
        databases,
        checks,
        logs,
        site,
    })
}

/// Tables of the site's local database on connection `name` (its `.env` prefix).
pub fn database(
    layout: &Layout,
    dir: &std::path::Path,
    name: &str,
    db_server: Option<&ipc::Service>,
) -> io::Result<SiteDatabase> {
    let config = Config::load(&layout.config_file())?;
    let settings = project::databases(dir)
        .into_iter()
        .find(|d| d.name == name && d.is_local_mysql(config.ports.mariadb) && !d.database.is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such database on PHPev's MariaDB"))?;
    let tables = match db_server {
        Some(server) => mariadb::tables(layout, &server.version, server.port, &settings.database)?,
        None => None,
    };
    Ok(SiteDatabase {
        exists: db_server.map(|_| tables.is_some()),
        bytes: tables.iter().flatten().map(|t| t.bytes).sum(),
        tables: tables.unwrap_or_default(),
        name: settings.name,
        database: settings.database,
        host: "127.0.0.1".into(),
        port: config.ports.mariadb,
        user: settings.username,
        mariadb_running: db_server.is_some(),
        mariadb_version: db_server.map(|s| s.version.clone()),
    })
}

/// `composer --version` of the phar the shim would run, cached per phar and
/// modification time (running PHP takes a moment).
fn composer_version(layout: &Layout, php_dir: &std::path::Path, php_version: &str) -> Option<String> {
    static CACHE: Mutex<Option<HashMap<PathBuf, (SystemTime, String)>>> = Mutex::new(None);
    let phar = composer::phar_for(&layout.composer_dir(), php_version);
    let modified = std::fs::metadata(&phar).and_then(|m| m.modified()).ok()?;
    if let Some((when, v)) = CACHE.lock().ok()?.get_or_insert_with(HashMap::new).get(&phar) {
        if *when == modified {
            return Some(v.clone());
        }
    }
    let out = std::process::Command::new(php_dir.join("php.exe"))
        .env("PHPRC", php::php_path(php_dir).ok()?)
        .arg(&phar)
        .args(["--version", "--no-ansi", "--no-interaction"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    // "Composer version 2.8.3 2024-11-17 13:13:04"
    let text = String::from_utf8_lossy(&out.stdout);
    let found = text
        .split_whitespace()
        .find(|w| version::parts(w).is_some_and(|p| p.len() >= 2))?
        .to_owned();
    CACHE
        .lock()
        .ok()?
        .get_or_insert_with(HashMap::new)
        .insert(phar, (modified, found.clone()));
    Some(found)
}

/// The site's local database on connection `name`: (database name, server).
pub fn local_database(layout: &Layout, dir: &std::path::Path, name: &str) -> io::Result<String> {
    let config = Config::load(&layout.config_file())?;
    project::databases(dir)
        .into_iter()
        .find(|d| d.name == name && d.is_local_mysql(config.ports.mariadb) && !d.database.is_empty())
        .map(|d| d.database)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such database on PHPev's MariaDB"))
}
