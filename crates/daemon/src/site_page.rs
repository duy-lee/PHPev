//! Data for the dashboard's site page: what the project is, how PHPev runs it
//! and whether it is ready. Blocking; call through `spawn_blocking`.

use std::io;
use std::path::PathBuf;

use phpev_core::api::{ComposerChoice, SiteDatabase, SiteDb, SiteDetail, SiteInfo};
use phpev_core::config::Config;
use phpev_core::layout::Layout;
use phpev_core::shim::{self, Source};
use phpev_core::{ca, composer, ipc, mariadb, php, project};

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
            defender_excluded: config.is_defender_excluded(&dir),
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

    let picked = resolved.as_ref().map(|r| {
        composer::pick(
            &layout.composer_dir(),
            &r.version,
            r.composer.as_deref(),
            r.composer_default.as_deref(),
        )
    });
    let composer_versions: Vec<ComposerChoice> = composer::installed(&layout.composer_dir())
        .into_iter()
        .map(|c| ComposerChoice {
            min_php: composer::min_php(&c).to_owned(),
            runs: php_version.as_deref().is_some_and(|v| composer::runs_on(&c, v)),
            version: c,
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
        composer: match &picked {
            Some(Ok(p)) => Some(p.version.clone()),
            _ => None,
        },
        composer_source: match &picked {
            Some(Ok(p)) => Some(
                match p.why {
                    composer::Why::Site => "site",
                    composer::Why::Default => "default",
                    composer::Why::Newest => "newest",
                }
                .into(),
            ),
            _ => None,
        },
        // Composer never installed is "Not installed", not an error.
        composer_error: match &picked {
            Some(Err(e)) if !composer_versions.is_empty() => Some(e.clone()),
            _ => None,
        },
        composer_versions,
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

/// The site's local database on connection `name`: (database name, server).
pub fn local_database(layout: &Layout, dir: &std::path::Path, name: &str) -> io::Result<String> {
    let config = Config::load(&layout.config_file())?;
    project::databases(dir)
        .into_iter()
        .find(|d| d.name == name && d.is_local_mysql(config.ports.mariadb) && !d.database.is_empty())
        .map(|d| d.database)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such database on PHPev's MariaDB"))
}
