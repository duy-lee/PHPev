//! Data for the dashboard's site page: what the project is, how PHPev runs it
//! and whether it is ready. Blocking; call through `spawn_blocking`.

use std::io;
use std::path::{Path, PathBuf};

use phpev_core::api::{AvailableDatabases, ComposerChoice, SiteDatabase, SiteDb, SiteDbEntry, SiteDetail, SiteInfo};
use phpev_core::config::Config;
use phpev_core::layout::Layout;
use phpev_core::shim::{self, Source};
use phpev_core::{ca, composer, ipc, mariadb, ops, php, project, sites};

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

    let record = ops::find_site(layout, &site.name)?;
    let (databases, site_databases) = site_databases(&config, &record);
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
        site_databases,
        docroot: relative(&dir, &record.docroot()),
        docroot_auto: relative(&dir, &record.auto_docroot()),
        docroot_picked: record.docroot.clone(),
        mariadb_version: db_server.map(|s| s.version.clone()),
        checks,
        logs,
        site,
    })
}

/// `dir` below `base` written the way `config.toml` keeps a document root (`.` for `base` itself).
fn relative(base: &Path, dir: &Path) -> String {
    match dir.strip_prefix(base) {
        Ok(rest) if !rest.as_os_str().is_empty() => rest.to_string_lossy().replace('\\', "/"),
        _ => ".".into(),
    }
}

/// The connections of the project in `dir`, and the databases of PHPev's MariaDB
/// that belong to it: the ones the connections name (a connection without a
/// database name has none to show), then those added by hand; the ones taken off
/// the tab are left out.
fn site_databases(config: &Config, site: &sites::Site) -> (Vec<SiteDb>, Vec<SiteDbEntry>) {
    let connections: Vec<SiteDb> = project::databases(&site.path)
        .into_iter()
        .map(|settings| SiteDb {
            local: settings.is_local_mysql(config.ports.mariadb) && !settings.database.is_empty(),
            settings,
        })
        .collect();
    let mut entries: Vec<SiteDbEntry> = Vec::new();
    let mut add = |entry: SiteDbEntry| {
        let name = entry.database.to_ascii_lowercase();
        if !site.hidden_databases.contains(&name) && !entries.iter().any(|e| e.database.eq_ignore_ascii_case(&name)) {
            entries.push(entry);
        }
    };
    for db in connections.iter().filter(|c| c.local) {
        add(SiteDbEntry {
            database: db.settings.database.clone(),
            user: if db.settings.username.is_empty() {
                "root".into()
            } else {
                db.settings.username.clone()
            },
            has_password: db.settings.has_password,
        });
    }
    for name in &site.databases {
        add(SiteDbEntry {
            database: name.clone(),
            user: "root".into(),
            has_password: false,
        });
    }
    (connections, entries)
}

/// The databases of PHPev's MariaDB that belong to `site` (see [`site_databases`]).
fn entries(layout: &Layout, site: &sites::Site) -> io::Result<(Config, Vec<SiteDbEntry>)> {
    let config = Config::load(&layout.config_file())?;
    let (_, entries) = site_databases(&config, site);
    Ok((config, entries))
}

/// The site's database `name` (any case), or `NotFound` when it is neither in
/// the project's settings nor added by hand: only those can be read, imported into or exported.
fn member(layout: &Layout, site: &sites::Site, name: &str) -> io::Result<(Config, SiteDbEntry)> {
    let (config, entries) = entries(layout, site)?;
    entries
        .into_iter()
        .find(|e| e.database.eq_ignore_ascii_case(name))
        .map(|e| (config, e))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("`{name}` is not a database of this site"),
            )
        })
}

/// Tables of the site's database `name`.
pub fn database(
    layout: &Layout,
    site: &sites::Site,
    name: &str,
    db_server: Option<&ipc::Service>,
) -> io::Result<SiteDatabase> {
    let (config, entry) = member(layout, site, name)?;
    let tables = match db_server {
        Some(server) => mariadb::tables(layout, &server.version, server.port, &entry.database)?,
        None => None,
    };
    Ok(SiteDatabase {
        exists: db_server.map(|_| tables.is_some()),
        bytes: tables.iter().flatten().map(|t| t.bytes).sum(),
        tables: tables.unwrap_or_default(),
        database: entry.database,
        host: "127.0.0.1".into(),
        port: config.ports.mariadb,
        user: entry.user,
        mariadb_running: db_server.is_some(),
        mariadb_version: db_server.map(|s| s.version.clone()),
    })
}

/// The name of the site's database `name`, for import and export.
pub fn local_database(layout: &Layout, site: &sites::Site, name: &str) -> io::Result<String> {
    member(layout, site, name).map(|(_, e)| e.database)
}

/// The database a `.env` connection (its prefix) names, for the "create it" fix.
pub fn connection_database(layout: &Layout, dir: &Path, connection: &str) -> io::Result<String> {
    let config = Config::load(&layout.config_file())?;
    project::databases(dir)
        .into_iter()
        .find(|d| d.name == connection && d.is_local_mysql(config.ports.mariadb) && !d.database.is_empty())
        .map(|d| d.database)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such database on PHPev's MariaDB"))
}

/// User databases of the running server that the site does not have yet.
pub fn available_databases(
    layout: &Layout,
    site: &sites::Site,
    db_server: Option<&ipc::Service>,
) -> io::Result<AvailableDatabases> {
    let Some(server) = db_server else {
        return Ok(AvailableDatabases {
            running: false,
            databases: Vec::new(),
        });
    };
    let (_, have) = entries(layout, site)?;
    let databases = mariadb::databases(layout, &server.version, server.port)?
        .into_iter()
        .filter(|db| !have.iter().any(|e| e.database.eq_ignore_ascii_case(&db.name)))
        .collect();
    Ok(AvailableDatabases {
        running: true,
        databases,
    })
}
