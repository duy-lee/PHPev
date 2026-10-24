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
pub fn server_args(layout: &Layout, version: &str, port: u16) -> Vec<std::ffi::OsString> {
    let major = major_of(version);
    let mut datadir = std::ffi::OsString::from("--datadir=");
    datadir.push(data_dir(layout, &major));
    let mut log = std::ffi::OsString::from("--log-error=");
    log.push(layout.logs_dir().join(format!("mariadb-{major}.err")));
    vec![
        "--no-defaults".into(),
        datadir,
        format!("--port={port}").into(),
        "--bind-address=127.0.0.1".into(),
        "--innodb-log-file-size=16M".into(),
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

/// A database name PHPev will put into SQL: letters, digits, `_`, `$`, `-`
/// (always backtick-quoted), at most 64 characters.
pub fn is_safe_db_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"_$-".contains(&b))
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
    fn major_from_version() {
        assert_eq!(major_of("11.4.13"), "11.4");
        assert_eq!(major_of("10.11.9"), "10.11");
    }
}
