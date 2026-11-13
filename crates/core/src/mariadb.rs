//! Installing MariaDB builds from the MariaDB downloads REST API.

use std::io;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::extract::{self, Stats};
use crate::layout::Layout;
use crate::net::{self, Http, Progress};
use crate::version;

const API: &str = "https://downloads.mariadb.org/rest-api/mariadb/";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Executables PHPev uses; everything else in `bin/` (backup tools, aria_*,
/// duplicated `mysql*` aliases) is left out. `mariadb-install-db` launches the
/// server as `mysqld.exe` (a 13 KB stub, like `mariadbd.exe`, around server.dll)
/// and `mariadb-upgrade` runs `mariadb-check`.
pub(crate) const KEEP_EXES: &[&str] = &[
    "mariadbd.exe",
    "mysqld.exe",
    "mariadb-install-db.exe",
    "mariadb-admin.exe",
    "mariadb.exe",
    "mariadb-dump.exe",
    "mariadb-upgrade.exe",
    "mariadb-check.exe",
];

#[derive(Clone, Debug, Deserialize)]
pub struct Major {
    pub release_id: String,
    pub release_status: String,
    pub release_support_type: Option<String>,
    pub release_eol_date: Option<String>,
}

impl Major {
    pub fn is_lts(&self) -> bool {
        self.release_support_type.as_deref() == Some("Long Term Support")
    }

    /// True if the end-of-life date (`YYYY-MM-DD`) is before `today` (same format).
    pub fn is_eol(&self, today: &str) -> bool {
        self.release_eol_date.as_deref().is_some_and(|d| d < today)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub major: String,
    pub version: String,
    pub url: String,
    pub sha256: String,
}

/// Stable major releases, newest first.
pub fn majors(http: &Http) -> io::Result<Vec<Major>> {
    #[derive(Deserialize)]
    struct Resp {
        major_releases: Vec<Major>,
    }
    let mut list = http.get_json::<Resp>(API)?.major_releases;
    list.retain(|m| m.release_status == "Stable");
    list.sort_by(|a, b| version::cmp(&b.release_id, &a.release_id));
    Ok(list)
}

/// Latest Windows x64 zip of a major release (e.g. `11.4`).
pub fn latest(http: &Http, major: &str) -> io::Result<Release> {
    let json: serde_json::Value = http.get_json(&format!("{API}{major}/latest/"))?;
    parse_latest(major, &json)
}

fn parse_latest(major: &str, json: &serde_json::Value) -> io::Result<Release> {
    #[derive(Deserialize)]
    struct Checksum {
        sha256sum: String,
    }
    #[derive(Deserialize)]
    struct File {
        file_download_url: String,
        checksum: Checksum,
    }

    let not_found = || {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("no Windows x64 zip for MariaDB {major}"),
        )
    };
    let releases = json.get("releases").and_then(|r| r.as_object()).ok_or_else(not_found)?;
    let (version, release) = releases
        .iter()
        .max_by(|a, b| version::cmp(a.0, b.0))
        .ok_or_else(not_found)?;
    // Other entries (repository folders, signatures) may carry `null` fields, so
    // only the Windows zip entry is deserialized.
    let zip = release
        .get("files")
        .and_then(|f| f.as_array())
        .into_iter()
        .flatten()
        .find(|f| {
            f.get("file_name")
                .and_then(|n| n.as_str())
                .is_some_and(|n| n.ends_with("-winx64.zip"))
        })
        .ok_or_else(not_found)?;
    let zip: File = serde_json::from_value(zip.clone()).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Release {
        major: major.to_owned(),
        version: version.clone(),
        url: net::force_https(&zip.file_download_url),
        sha256: zip.checksum.sha256sum,
    })
}

/// Installed versions (directories with every kept executable), newest first.
pub fn installed(layout: &Layout) -> Vec<String> {
    let mut versions: Vec<String> = std::fs::read_dir(layout.mariadb_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| is_complete(&e.path()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| version::parts(name).is_some())
        .collect();
    versions.sort_by(|a, b| version::cmp(b, a));
    versions
}

pub fn version_dir(layout: &Layout, version: &str) -> PathBuf {
    layout.mariadb_dir().join(version)
}

/// True if `dir` holds every executable PHPev needs. An install made by an
/// older PHPev with a shorter allowlist is incomplete and gets reinstalled.
fn is_complete(dir: &Path) -> bool {
    KEEP_EXES.iter().all(|exe| dir.join("bin").join(exe).is_file())
}

/// Data directory for a major version; datadirs are not shared across majors (§0).
pub fn data_dir(layout: &Layout, major: &str) -> PathBuf {
    layout.mariadb_data_dir().join(major)
}

/// Newest installed version of `major` (e.g. `11.4` → `11.4.13`).
pub fn installed_for_major(layout: &Layout, major: &str) -> Option<String> {
    let installed = installed(layout);
    version::best_match(major, installed.iter().map(String::as_str)).map(str::to_owned)
}

/// Arguments for `mariadbd.exe`. `--no-defaults` must come first; the 16 MB redo
/// log keeps a fresh datadir small (MariaDB resizes an existing log on start).
/// The my.ini of a major's server, next to its databases (written by
/// mariadb-install-db, editable in the dashboard).
pub fn ini_path(layout: &Layout, major: &str) -> PathBuf {
    data_dir(layout, major).join("my.ini")
}

/// Arguments of the server. The major's my.ini is read first; the arguments
/// after it win, so PHPev keeps the data folder, port, address and logs.
pub fn server_args(layout: &Layout, version: &str, port: u16) -> Vec<std::ffi::OsString> {
    let major = major_of(version);
    let ini = ini_path(layout, &major);
    let defaults = if ini.is_file() {
        let mut arg = std::ffi::OsString::from("--defaults-file=");
        arg.push(&ini);
        arg
    } else {
        "--no-defaults".into()
    };
    let mut datadir = std::ffi::OsString::from("--datadir=");
    datadir.push(data_dir(layout, &major));
    let mut log = std::ffi::OsString::from("--log-error=");
    log.push(layout.logs_dir().join(format!("mariadb-{major}.err")));
    vec![
        defaults,
        datadir,
        format!("--port={port}").into(),
        "--bind-address=127.0.0.1".into(),
        "--innodb-log-file-size=16M".into(),
        // Dumps with big rows or long INSERTs import without "packet too big".
        crate::dbdump::MAX_PACKET.into(),
        log,
    ]
}

pub fn server_exe(layout: &Layout, version: &str) -> PathBuf {
    version_dir(layout, version).join(r"bin\mariadbd.exe")
}

/// Asks the server to shut down cleanly (flushes InnoDB; no crash recovery on next start).
pub fn admin_shutdown(layout: &Layout, version: &str, port: u16) -> io::Result<()> {
    let exe = version_dir(layout, version).join(r"bin\mariadb-admin.exe");
    let out = Command::new(exe)
        .args([
            "--no-defaults",
            "--user=root",
            "--host=127.0.0.1",
            &format!("--port={port}"),
            "shutdown",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(String::from_utf8_lossy(&out.stderr).trim().to_owned()))
    }
}

/// Runs SQL as root through the bundled client (`mariadb.exe -e`).
pub fn exec_sql(layout: &Layout, version: &str, port: u16, sql: &str) -> io::Result<()> {
    let exe = version_dir(layout, version).join(r"bin\mariadb.exe");
    let out = Command::new(exe)
        .args([
            "--no-defaults",
            "--user=root",
            "--host=127.0.0.1",
            &format!("--port={port}"),
            "-e",
            sql,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(String::from_utf8_lossy(&out.stderr).trim().to_owned()))
    }
}

/// Runs a query as root and returns its rows (tab-separated batch output).
pub fn query(layout: &Layout, version: &str, port: u16, sql: &str) -> io::Result<Vec<Vec<String>>> {
    let exe = version_dir(layout, version).join(r"bin\mariadb.exe");
    let out = Command::new(exe)
        .args([
            "--no-defaults",
            "--user=root",
            "--host=127.0.0.1",
            &format!("--port={port}"),
            "--batch",
            "--skip-column-names",
            "-e",
            sql,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if !out.status.success() {
        return Err(io::Error::other(String::from_utf8_lossy(&out.stderr).trim().to_owned()));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.split('\t').map(str::to_owned).collect())
        .collect())
}

/// One table of a database, as the dashboard lists it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TableInfo {
    pub name: String,
    /// InnoDB's estimate, exact for small tables.
    pub rows: u64,
    /// Data and indexes.
    pub bytes: u64,
}

/// Every database on the server.
pub fn database_names(layout: &Layout, version: &str, port: u16) -> io::Result<Vec<String>> {
    Ok(query(layout, version, port, "SHOW DATABASES")?
        .into_iter()
        .filter_map(|r| r.into_iter().next())
        .collect())
}

pub fn database_exists(layout: &Layout, version: &str, port: u16, db: &str) -> io::Result<bool> {
    if !is_safe_db_name(db) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported database name `{db}`"),
        ));
    }
    let rows = query(
        layout,
        version,
        port,
        &format!("SELECT 1 FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = '{db}'"),
    )?;
    Ok(!rows.is_empty())
}

/// Tables of `db`, largest first; `None` if the database does not exist.
pub fn tables(layout: &Layout, version: &str, port: u16, db: &str) -> io::Result<Option<Vec<TableInfo>>> {
    if !is_safe_db_name(db) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported database name `{db}`"),
        ));
    }
    if !database_exists(layout, version, port, db)? {
        return Ok(None);
    }
    let rows = query(
        layout,
        version,
        port,
        &format!(
            "SELECT TABLE_NAME, IFNULL(TABLE_ROWS, 0), IFNULL(DATA_LENGTH, 0) + IFNULL(INDEX_LENGTH, 0) \
             FROM information_schema.TABLES WHERE TABLE_SCHEMA = '{db}' ORDER BY 3 DESC, 1"
        ),
    )?;
    Ok(Some(
        rows.into_iter()
            .filter_map(|r| match r.as_slice() {
                [name, rows, bytes] => Some(TableInfo {
                    name: name.clone(),
                    rows: rows.parse().unwrap_or(0),
                    bytes: bytes.parse().unwrap_or(0),
                }),
                _ => None,
            })
            .collect(),
    ))
}

/// A database name PHPev will put into SQL: letters, digits, `_`, `$`, `-`
/// (always backtick-quoted), at most 64 characters.
pub fn is_safe_db_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"_$-".contains(&b))
}

/// Databases of the server itself: never listed, never created or dropped by
/// PHPev (`test` is the empty one `mariadb-install-db` leaves behind).
const SYSTEM_DATABASES: &[&str] = &["mysql", "sys", "information_schema", "performance_schema", "test"];

pub fn is_system_db(name: &str) -> bool {
    SYSTEM_DATABASES.iter().any(|s| s.eq_ignore_ascii_case(name))
}

/// One database as the Services page lists it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DatabaseInfo {
    pub name: String,
    pub tables: u64,
    /// Data and indexes.
    pub bytes: u64,
}

/// User databases of the running server with their table count and size, by name.
pub fn databases(layout: &Layout, version: &str, port: u16) -> io::Result<Vec<DatabaseInfo>> {
    let system = SYSTEM_DATABASES
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(", ");
    let rows = query(
        layout,
        version,
        port,
        &format!(
            "SELECT s.SCHEMA_NAME, COUNT(t.TABLE_NAME), IFNULL(SUM(t.DATA_LENGTH + t.INDEX_LENGTH), 0) \
             FROM information_schema.SCHEMATA s \
             LEFT JOIN information_schema.TABLES t ON t.TABLE_SCHEMA = s.SCHEMA_NAME \
             WHERE s.SCHEMA_NAME NOT IN ({system}) GROUP BY s.SCHEMA_NAME ORDER BY s.SCHEMA_NAME"
        ),
    )?;
    Ok(rows
        .into_iter()
        .filter_map(|r| match r.as_slice() {
            [name, tables, bytes] => Some(DatabaseInfo {
                name: name.clone(),
                tables: tables.parse().unwrap_or(0),
                bytes: bytes.parse().unwrap_or(0),
            }),
            _ => None,
        })
        .collect())
}

/// User databases of a major that is not running, read from its data folder.
/// The table count and size are estimates (files, not the server's statistics).
pub fn databases_on_disk(layout: &Layout, major: &str) -> Vec<DatabaseInfo> {
    databases_in(&data_dir(layout, major))
}

fn databases_in(data: &Path) -> Vec<DatabaseInfo> {
    let mut found: Vec<DatabaseInfo> = std::fs::read_dir(data)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let dir = e.file_name().into_string().ok()?;
            // `#innodb_redo`, `#innodb_temp` and the like are not databases.
            if dir.starts_with('#') {
                return None;
            }
            let name = decode_file_name(&dir);
            if is_system_db(&name) {
                return None;
            }
            let (tables, bytes) = scan_database_dir(&e.path());
            Some(DatabaseInfo { name, tables, bytes })
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// Tables (distinct file stems of table files, partitions counted once) and
/// total file size of a database folder.
fn scan_database_dir(dir: &Path) -> (u64, u64) {
    let mut stems = std::collections::HashSet::new();
    let mut bytes = 0;
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        bytes += entry.metadata().map_or(0, |m| m.len());
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some((stem, ext)) = name.rsplit_once('.') else {
            continue;
        };
        if stem.starts_with('#') || !["frm", "ibd", "myd", "csv"].iter().any(|e| ext.eq_ignore_ascii_case(e)) {
            continue;
        }
        stems.insert(stem.split("#P#").next().unwrap_or(stem).to_ascii_lowercase());
    }
    (stems.len() as u64, bytes)
}

/// The server writes characters outside `[A-Za-z0-9_]` in folder names as `@`
/// and four hex digits (`my-db` → `my@002ddb`).
fn decode_file_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut rest = name;
    while let Some(at) = rest.find('@') {
        out.push_str(&rest[..at]);
        let decoded = rest
            .get(at + 1..at + 5)
            .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()))
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .and_then(char::from_u32);
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[at + 5..];
            }
            None => {
                out.push('@');
                rest = &rest[at + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Creates database `name` on the running server (lower-cased: the server does
/// that on Windows anyway). Returns the name as created.
pub fn create_database(layout: &Layout, version: &str, port: u16, name: &str) -> io::Result<String> {
    let name = name.trim().to_ascii_lowercase();
    if !is_safe_db_name(&name) || is_system_db(&name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported database name `{name}`: use letters, digits, _ $ - (at most 64)"),
        ));
    }
    if database_exists(layout, version, port, &name)? {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("database `{name}` already exists"),
        ));
    }
    exec_sql(
        layout,
        version,
        port,
        &format!("CREATE DATABASE `{name}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci"),
    )?;
    Ok(name)
}

/// `name` if it is a user database that exists on the running server (so a
/// dump goes into, or out of, a database the user can see in the list).
pub fn user_database(layout: &Layout, version: &str, port: u16, name: &str) -> io::Result<String> {
    if !is_safe_db_name(name) || is_system_db(name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`{name}` is not a database PHPev works with"),
        ));
    }
    if !database_exists(layout, version, port, name)? {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("database `{name}` does not exist"),
        ));
    }
    Ok(name.to_owned())
}

/// Drops database `name` from the running server, with everything in it.
pub fn drop_database(layout: &Layout, version: &str, port: u16, name: &str) -> io::Result<()> {
    if !is_safe_db_name(name) || is_system_db(name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`{name}` cannot be dropped here"),
        ));
    }
    if !database_exists(layout, version, port, name)? {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("database `{name}` does not exist"),
        ));
    }
    exec_sql(layout, version, port, &format!("DROP DATABASE `{name}`"))
}

/// `11.4.13` → `11.4`
pub fn major_of(version: &str) -> String {
    version.split('.').take(2).collect::<Vec<_>>().join(".")
}

/// Downloads and unpacks `release`. Returns `None` if it was already installed.
pub fn install(http: &Http, layout: &Layout, release: &Release, progress: Progress<'_>) -> io::Result<Option<Stats>> {
    let dir = version_dir(layout, &release.version);
    if is_complete(&dir) {
        return Ok(None);
    }
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(layout.downloads_dir())?;
    let zip = layout
        .downloads_dir()
        .join(format!("mariadb-{}-winx64.zip", release.version));
    net::fetch_verified(http, &release.url, &zip, &release.sha256, progress)?;

    let mut stats = Stats::default();
    extract::install_atomically(&dir, |tmp| {
        stats = extract::extract_zip(&zip, tmp, true, &keep_mariadb_entry)?;
        Ok(())
    })?;
    std::fs::remove_file(&zip)?;
    Ok(Some(stats))
}

fn keep_mariadb_entry(path: &str) -> bool {
    if let Some(name) = path.strip_prefix("bin/") {
        return !name.contains('/') && (name.ends_with(".dll") || KEEP_EXES.contains(&name));
    }
    path.starts_with("share/") || (path.starts_with("lib/plugin/") && path.ends_with(".dll"))
}

/// Creates the data directory for `version`'s major with `mariadb-install-db`
/// unless it already exists. Returns whether it was created.
pub fn init_data_dir(layout: &Layout, version: &str) -> io::Result<bool> {
    let data = data_dir(layout, &major_of(version));
    if data.join("mysql").is_dir() {
        return Ok(false);
    }
    if data.exists() {
        std::fs::remove_dir_all(&data)?; // leftover of an interrupted init
    }
    std::fs::create_dir_all(layout.mariadb_data_dir())?;
    let exe = version_dir(layout, version).join(r"bin\mariadb-install-db.exe");
    let mut arg = std::ffi::OsString::from("--datadir=");
    arg.push(&data);
    let out = Command::new(&exe).arg(arg).creation_flags(CREATE_NO_WINDOW).output()?;
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(&data);
        return Err(io::Error::other(format!(
            "mariadb-install-db failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist() {
        assert!(keep_mariadb_entry("bin/mariadbd.exe"));
        assert!(keep_mariadb_entry("bin/mysqld.exe"));
        assert!(keep_mariadb_entry("bin/server.dll"));
        assert!(keep_mariadb_entry("bin/vcruntime140.dll"));
        assert!(keep_mariadb_entry("share/english/errmsg.sys"));
        assert!(keep_mariadb_entry("lib/plugin/auth_ed25519.dll"));
        assert!(!keep_mariadb_entry("bin/server.pdb"));
        assert!(!keep_mariadb_entry("bin/mariadb-backup.exe"));
        assert!(!keep_mariadb_entry("bin/mysqldump.exe"));
        assert!(!keep_mariadb_entry("lib/plugin/ha_connect.lib"));
        assert!(!keep_mariadb_entry("include/mysql/mysql.h"));
    }

    #[test]
    fn parses_latest_release() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"releases": {"11.4.13": {"release_id": "11.4.13", "files": [
                {"file_name": "mariadb-11.4.13.tar.gz", "file_download_url": "http://x/src", "checksum": {"sha256sum": "aa"}},
                {"file_name": "yum/", "file_download_url": null, "checksum": {"sha256sum": null}},
                {"file_name": "mariadb-11.4.13-winx64.zip", "file_download_url": "http://downloads.mariadb.org/rest-api/mariadb/11.4.13/mariadb-11.4.13-winx64.zip", "checksum": {"sha256sum": "bb", "md5sum": "cc"}}
            ]}}}"#,
        )
        .unwrap();
        let r = parse_latest("11.4", &json).unwrap();
        assert_eq!(r.version, "11.4.13");
        assert_eq!(r.sha256, "bb");
        assert!(r.url.starts_with("https://downloads.mariadb.org/"));
    }

    #[test]
    fn folder_names_decode() {
        assert_eq!(decode_file_name("shop_main"), "shop_main");
        assert_eq!(decode_file_name("my@002ddb"), "my-db");
        assert_eq!(decode_file_name("a@0024b@002dc"), "a$b-c");
        // Not an escape: kept as it is.
        assert_eq!(decode_file_name("a@zz"), "a@zz");
        assert_eq!(decode_file_name("end@"), "end@");
    }

    #[test]
    fn system_databases_are_recognised() {
        assert!(is_system_db("mysql") && is_system_db("Performance_Schema") && is_system_db("test"));
        assert!(!is_system_db("tests") && !is_system_db("shop"));
    }

    #[test]
    fn databases_are_read_from_a_data_folder() {
        let root = std::env::temp_dir().join(format!("phpev-dbs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let put = |rel: &str, len: usize| {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, vec![0u8; len]).unwrap();
        };
        put("shop/orders.frm", 10);
        put("shop/orders.ibd", 100);
        put("shop/users.ibd", 50);
        put("shop/logs#P#p0.ibd", 5);
        put("shop/logs#P#p1.ibd", 5);
        put("shop/db.opt", 1);
        put("my@002ddb/t.ibd", 7);
        put("empty/db.opt", 1);
        put("mysql/user.frm", 1);
        put("test/db.opt", 1);
        put("#innodb_redo/ib_redo1", 1);
        put("ibdata1", 1000);

        let found = databases_in(&root);
        let summary: Vec<(&str, u64, u64)> = found.iter().map(|d| (d.name.as_str(), d.tables, d.bytes)).collect();
        assert_eq!(summary, [("empty", 0, 1), ("my-db", 1, 7), ("shop", 3, 171)]);
        assert!(databases_in(&root.join("missing")).is_empty());

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Needs PHPev's MariaDB running (`cargo test -- --ignored`); creates and drops `phpev_db_test`.
    #[test]
    #[ignore]
    fn create_list_and_drop_against_the_running_server() {
        use crate::config::Config;
        let layout = Layout::resolve().unwrap();
        let port = Config::load(&layout.config_file()).unwrap().ports.mariadb;
        let version = installed(&layout).into_iter().next().expect("MariaDB installed");

        let created = create_database(&layout, &version, port, " PHPEV_DB_Test ").unwrap();
        assert_eq!(created, "phpev_db_test");
        let err = create_database(&layout, &version, port, "phpev_db_test").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert!(databases(&layout, &version, port)
            .unwrap()
            .iter()
            .any(|d| d.name == created && d.tables == 0));

        assert_eq!(user_database(&layout, &version, port, &created).unwrap(), created);
        assert!(user_database(&layout, &version, port, "mysql").is_err());

        drop_database(&layout, &version, port, &created).unwrap();
        assert_eq!(
            user_database(&layout, &version, port, &created).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            drop_database(&layout, &version, port, &created).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert!(create_database(&layout, &version, port, "mysql").is_err());
        assert!(drop_database(&layout, &version, port, "mysql").is_err());
    }

    #[test]
    fn major_from_version() {
        assert_eq!(major_of("11.4.13"), "11.4");
        assert_eq!(major_of("10.11.9"), "10.11");
    }
}
