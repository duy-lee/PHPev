//! Installing PHP builds from windows.php.net and generating their php.ini.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::extract::{self, Stats};
use crate::layout::{ascii_short_path, Layout};
use crate::net::{self, Http, Progress};
use crate::registry::{self, Hive};
use crate::{elevate, trust, version};

pub const RELEASES_URL: &str = "https://downloads.php.net/~windows/releases/releases.json";
const DOWNLOAD_BASE: &str = "https://downloads.php.net/~windows/releases/";
const CACERT_URL: &str = "https://curl.se/ca/cacert.pem";
const VC_REDIST_URL: &str = "https://aka.ms/vs/17/release/vc_redist.x64.exe";

/// Extensions enabled in the generated php.ini, when the build ships them as DLLs
/// (some, like bcmath on 8.4, are compiled in and must not be listed).
const EXTENSIONS: &[&str] = &[
    "openssl",
    "curl",
    "mbstring",
    "fileinfo",
    "intl",
    "pdo_mysql",
    "mysqli",
    "pdo_sqlite",
    "sqlite3",
    "zip",
    "gd",
    "sodium",
    "exif",
    "bcmath",
];

/// One downloadable NTS x64 build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// Release line, e.g. `8.4`.
    pub line: String,
    /// Full version, e.g. `8.4.27`.
    pub version: String,
    /// File name under the releases directory.
    pub zip: String,
    pub sha256: String,
    /// Visual C++ toolset the build needs (`vs16`, `vs17`).
    pub toolset: String,
}

/// Parses releases.json into the NTS x64 builds, newest first.
pub fn parse_releases(json: &str) -> io::Result<Vec<Release>> {
    #[derive(Deserialize)]
    struct Zip {
        path: String,
        sha256: String,
    }
    #[derive(Deserialize)]
    struct Build {
        zip: Zip,
    }

    let lines: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(json).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut out = Vec::new();
    for (line, entry) in lines {
        let Some(obj) = entry.as_object() else { continue };
        let Some(version) = obj.get("version").and_then(|v| v.as_str()) else {
            continue;
        };
        // Keys look like `nts-vs17-x64`; the toolset changes between lines, so don't hard-code it.
        let Some((key, build)) = obj.iter().find(|(k, _)| k.starts_with("nts-") && k.ends_with("-x64")) else {
            continue;
        };
        let Ok(build) = serde_json::from_value::<Build>(build.clone()) else {
            continue;
        };
        out.push(Release {
            line: line.clone(),
            version: version.to_owned(),
            zip: build.zip.path,
            sha256: build.zip.sha256,
            toolset: key.trim_start_matches("nts-").trim_end_matches("-x64").to_owned(),
        });
    }
    out.sort_by(|a, b| version::cmp(&b.version, &a.version));
    Ok(out)
}

pub fn available(http: &Http) -> io::Result<Vec<Release>> {
    parse_releases(&http.get_text(RELEASES_URL)?)
}

/// Picks the release for `wanted` (`8.4` or an exact `8.4.27`).
pub fn resolve<'a>(releases: &'a [Release], wanted: &str) -> io::Result<&'a Release> {
    let found = releases
        .iter()
        .filter(|r| version::matches(wanted, &r.version))
        .max_by(|a, b| version::cmp(&a.version, &b.version));
    found.ok_or_else(|| {
        let lines: Vec<&str> = releases.iter().map(|r| r.version.as_str()).collect();
        io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "PHP {wanted} is not available (current builds: {}). Older patch releases are not supported yet.",
                lines.join(", ")
            ),
        )
    })
}

/// Installed versions (directories containing php.exe), newest first.
pub fn installed(layout: &Layout) -> Vec<String> {
    let mut versions: Vec<String> = std::fs::read_dir(layout.php_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("php.exe").is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| version::parts(name).is_some())
        .collect();
    versions.sort_by(|a, b| version::cmp(b, a));
    versions
}

pub fn version_dir(layout: &Layout, version: &str) -> PathBuf {
    layout.php_dir().join(version)
}

/// Downloads and unpacks `release`. Returns `None` if it was already installed.
pub fn install(http: &Http, layout: &Layout, release: &Release, progress: Progress<'_>) -> io::Result<Option<Stats>> {
    let dir = version_dir(layout, &release.version);
    if dir.join("php.exe").is_file() {
        return Ok(None);
    }
    std::fs::create_dir_all(layout.downloads_dir())?;
    let zip = layout.downloads_dir().join(&release.zip);
    let url = format!("{DOWNLOAD_BASE}{}", release.zip);
    net::fetch_verified(http, &url, &zip, &release.sha256, progress)?;

    let mut stats = Stats::default();
    extract::install_atomically(&dir, |tmp| {
        stats = extract::extract_zip(&zip, tmp, false, &keep_php_entry)?;
        Ok(())
    })?;
    std::fs::remove_file(&zip)?;
    Ok(Some(stats))
}

/// Drops the SDK (`dev/`) and debug symbols; everything else is needed at runtime
/// (`extras/ssl/openssl.cnf` is used by the openssl extension).
fn keep_php_entry(path: &str) -> bool {
    !path.starts_with("dev/") && !path.ends_with(".pdb")
}

/// Writes `php.ini` for an installed version unless one already exists, so
/// user edits survive. Returns whether a file was written.
pub fn write_php_ini(layout: &Layout, version: &str) -> io::Result<bool> {
    let dir = version_dir(layout, version);
    let ini = dir.join("php.ini");
    if ini.exists() {
        return Ok(false);
    }
    std::fs::create_dir_all(layout.logs_dir())?;
    let text = render_php_ini(
        &php_path(&dir.join("ext"))?,
        &php_path(&layout.cacert())?,
        &php_path(&layout.logs_dir())?.join(format!("php-{version}.log")),
        |ext| dir.join("ext").join(format!("php_{ext}.dll")).is_file(),
    );
    std::fs::write(ini, text)?;
    Ok(true)
}

/// Extensions a Laravel app cannot run without; `verify` insists on them.
const REQUIRED_MODULES: &[&str] = &["openssl", "curl", "mbstring", "fileinfo", "pdo_mysql", "Zend OPcache"];

/// Runs the installed PHP with its php.ini and returns the loaded modules.
/// Fails on startup warnings (e.g. an extension that cannot load) or if a
/// required extension is missing.
pub fn verify(layout: &Layout, version: &str) -> io::Result<Vec<String>> {
    let modules = modules(layout, version)?;
    let missing: Vec<&str> = REQUIRED_MODULES
        .iter()
        .copied()
        .filter(|m| !modules.iter().any(|x| x == m))
        .collect();
    if !missing.is_empty() {
        return Err(io::Error::other(format!(
            "PHP {version} is missing required extensions: {} (check {})",
            missing.join(", "),
            version_dir(layout, version).join("php.ini").display()
        )));
    }
    Ok(modules)
}

/// Loaded modules as `php -m` prints them (e.g. `Zend OPcache`, `SimpleXML`).
/// Fails on startup warnings such as an extension DLL that cannot load.
pub fn modules(layout: &Layout, version: &str) -> io::Result<Vec<String>> {
    let dir = version_dir(layout, version);
    let ini = php_path(&dir.join("php.ini"))?;
    let out = std::process::Command::new(dir.join("php.exe"))
        .arg("-c")
        .arg(&ini)
        .arg("-m")
        .output()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    let problems: Vec<&str> = stdout
        .lines()
        .chain(stderr.lines())
        .filter(|l| l.contains("Warning") || l.contains("Fatal") || l.contains("Unable to load"))
        .collect();
    if !out.status.success() || !problems.is_empty() {
        return Err(io::Error::other(format!(
            "PHP {version} failed its self-check ({}):\n  {}",
            out.status,
            problems.join("\n  ")
        )));
    }

    Ok(stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('['))
        .map(str::to_owned)
        .collect())
}

/// True if the build ships `ext` as a loadable DLL (`ext\php_<ext>.dll`).
pub fn has_extension_dll(layout: &Layout, version: &str, ext: &str) -> bool {
    version_dir(layout, version)
        .join("ext")
        .join(format!("php_{ext}.dll"))
        .is_file()
}

/// True for an active `extension=<ext>` line (also `php_<ext>.dll`, quoted).
fn is_extension_line(line: &str, ext: &str) -> bool {
    let l = line.trim();
    !l.starts_with(';')
        && l.split_once('=').is_some_and(|(k, v)| {
            k.trim() == "extension"
                && v.trim()
                    .trim_matches('"')
                    .trim_start_matches("php_")
                    .trim_end_matches(".dll")
                    .eq_ignore_ascii_case(ext)
        })
}

/// Comments out every `extension=<ext>` line in the version's php.ini.
/// Returns false if it was not enabled there.
pub fn disable_extension(layout: &Layout, version: &str, ext: &str) -> io::Result<bool> {
    let ini = version_dir(layout, version).join("php.ini");
    let text = std::fs::read_to_string(&ini)?;
    if !text.lines().any(|l| is_extension_line(l, ext)) {
        return Ok(false);
    }
    let mut out: String = text
        .lines()
        .map(|l| {
            if is_extension_line(l, ext) {
                format!(";{l}")
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    out.push('\n');
    std::fs::write(ini, out)?;
    Ok(true)
}

/// Appends `extension=<ext>` to the version's php.ini (it applies to every
/// project using that version). Returns false if it was already enabled.
pub fn enable_extension(layout: &Layout, version: &str, ext: &str) -> io::Result<bool> {
    let ini = version_dir(layout, version).join("php.ini");
    let text = std::fs::read_to_string(&ini)?;
    if text.lines().any(|l| is_extension_line(l, ext)) {
        return Ok(false);
    }
    let mut text = text;
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!("; enabled by `phpev import`\nextension={ext}\n"));
    std::fs::write(ini, text)?;
    Ok(true)
}

/// PHP cannot open non-ASCII paths from php.ini (spike S3): hand it the 8.3 short
/// name instead. The path must exist.
pub fn php_path(path: &Path) -> io::Result<PathBuf> {
    if path.to_str().is_some_and(str::is_ascii) {
        return Ok(path.to_owned());
    }
    ascii_short_path(path).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} is not ASCII and has no 8.3 short name; PHP cannot use it. Run `phpev setup --home C:\\PHPev`.",
                path.display()
            ),
        )
    })
}

fn render_php_ini(ext_dir: &Path, cacert: &Path, error_log: &Path, has_ext: impl Fn(&str) -> bool) -> String {
    let mut s = String::new();
    s.push_str("; Generated by PHPev for local development. PHPev never overwrites this file;\n");
    s.push_str("; delete it and run `phpev php ini <version>` to regenerate.\n\n");
    s.push_str("[PHP]\n");
    for (k, v) in [
        ("memory_limit", "512M"),
        ("max_execution_time", "60"),
        ("max_input_time", "60"),
        ("max_input_vars", "5000"),
        ("post_max_size", "128M"),
        ("upload_max_filesize", "128M"),
        ("max_file_uploads", "50"),
        ("error_reporting", "E_ALL"),
        ("display_errors", "On"),
        ("display_startup_errors", "On"),
        ("log_errors", "On"),
        ("variables_order", "\"GPCS\""),
        ("request_order", "\"GP\""),
        ("default_charset", "\"UTF-8\""),
        ("short_open_tag", "Off"),
        ("expose_php", "Off"),
        ("realpath_cache_size", "4096K"),
        ("realpath_cache_ttl", "600"),
        ("date.timezone", "UTC"),
    ] {
        s.push_str(&format!("{k}={v}\n"));
    }
    s.push_str(&format!("error_log=\"{}\"\n", error_log.display()));
    s.push_str(&format!("extension_dir=\"{}\"\n\n", ext_dir.display()));

    for ext in EXTENSIONS.iter().filter(|e| has_ext(e)) {
        s.push_str(&format!("extension={ext}\n"));
    }
    s.push_str("\n[curl]\n");
    s.push_str(&format!("curl.cainfo=\"{}\"\n", cacert.display()));
    s.push_str("\n[openssl]\n");
    s.push_str(&format!("openssl.cafile=\"{}\"\n", cacert.display()));

    s.push_str("\n[opcache]\n");
    // From PHP 8.5 OPcache is built in and no longer a loadable DLL.
    if has_ext("opcache") {
        s.push_str("zend_extension=opcache\n");
    }
    for (k, v) in [
        ("opcache.enable", "1"),
        ("opcache.enable_cli", "0"),
        ("opcache.memory_consumption", "256"),
        ("opcache.interned_strings_buffer", "16"),
        ("opcache.max_accelerated_files", "20000"),
        ("opcache.validate_timestamps", "1"),
        ("opcache.revalidate_freq", "0"),
    ] {
        s.push_str(&format!("{k}={v}\n"));
    }
    s
}

/// Downloads the curl CA bundle PHP needs for HTTPS (Windows PHP ignores the
/// system certificate store). Returns whether it was downloaded.
pub fn ensure_cacert(http: &Http, layout: &Layout, progress: Progress<'_>) -> io::Result<bool> {
    let dest = layout.cacert();
    if dest.is_file() {
        return Ok(false);
    }
    let sha = http.get_text(&format!("{CACERT_URL}.sha256"))?;
    let sha = net::parse_checksum_file(&sha)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unexpected cacert.pem.sha256 format"))?
        .to_owned();
    std::fs::create_dir_all(dest.parent().unwrap_or(layout.root()))?;
    http.download(CACERT_URL, &dest, Some(&sha), progress)?;
    Ok(true)
}

/// Minimum VC++ runtime (major, minor) for a PHP toolset.
fn required_vc_runtime(toolset: &str) -> (u32, u32) {
    match toolset {
        "vs17" => (14, 30),
        _ => (14, 20),
    }
}

/// Installed VC++ 2015-2022 x64 runtime version as (major, minor), if any.
pub fn vc_runtime_version() -> Option<(u32, u32)> {
    const KEY: &str = r"SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64";
    if registry::read_dword(Hive::LocalMachine, KEY, "Installed")
        .ok()
        .flatten()
        != Some(1)
    {
        return None;
    }
    let major = registry::read_dword(Hive::LocalMachine, KEY, "Major").ok().flatten()?;
    let minor = registry::read_dword(Hive::LocalMachine, KEY, "Minor").ok().flatten()?;
    Some((major, minor))
}

pub fn vc_runtime_ok(toolset: &str) -> bool {
    vc_runtime_version().is_some_and(|v| v >= required_vc_runtime(toolset))
}

/// Downloads Microsoft's VC++ redistributable and runs it through UAC.
/// Microsoft publishes no hash for it, so its Authenticode signature must be
/// valid and issued to Microsoft Corporation before it is started.
pub fn install_vc_runtime(http: &Http, layout: &Layout, progress: Progress<'_>) -> io::Result<()> {
    std::fs::create_dir_all(layout.downloads_dir())?;
    let exe = layout.downloads_dir().join("vc_redist.x64.exe");
    http.download(VC_REDIST_URL, &exe, None, progress)?;
    if let Err(e) = trust::require_microsoft(&exe) {
        let _ = std::fs::remove_file(&exe);
        return Err(e);
    }
    let code = elevate::run_elevated(&exe, &["/install", "/quiet", "/norestart"]);
    let _ = std::fs::remove_file(&exe);
    match code? {
        // 3010: success, reboot required; 1638: a newer version is already installed.
        0 | 3010 | 1638 => Ok(()),
        c => Err(io::Error::other(format!(
            "VC++ redistributable installer failed (exit code {c})"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "8.3": {"version": "8.3.36",
                "nts-vs16-x64": {"mtime": "x", "zip": {"path": "php-8.3.36-nts-Win32-vs16-x64.zip", "size": "1", "sha256": "aa"}},
                "ts-vs16-x64": {"zip": {"path": "ts.zip", "size": "1", "sha256": "bb"}}},
        "8.4": {"version": "8.4.27",
                "ts-vs17-x64": {"zip": {"path": "ts.zip", "size": "1", "sha256": "cc"}},
                "nts-vs17-x64": {"zip": {"path": "php-8.4.27-nts-Win32-vs17-x64.zip", "size": "1", "sha256": "dd"}},
                "source": {"path": "php-8.4.27-src.zip", "size": "1", "sha256": "ee"}}
    }"#;

    #[test]
    fn parses_nts_x64_builds_newest_first() {
        let r = parse_releases(SAMPLE).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].version, "8.4.27");
        assert_eq!(r[0].zip, "php-8.4.27-nts-Win32-vs17-x64.zip");
        assert_eq!(r[0].sha256, "dd");
        assert_eq!(r[0].toolset, "vs17");
        assert_eq!(r[1].toolset, "vs16");
    }

    #[test]
    fn resolves_line_or_exact_version() {
        let r = parse_releases(SAMPLE).unwrap();
        assert_eq!(resolve(&r, "8.4").unwrap().version, "8.4.27");
        assert_eq!(resolve(&r, "8.3.36").unwrap().version, "8.3.36");
        assert!(resolve(&r, "8.4.1").is_err());
    }

    #[test]
    fn ini_lists_only_shipped_extensions() {
        let ini = render_php_ini(
            Path::new(r"C:\p\ext"),
            Path::new(r"C:\c.pem"),
            Path::new(r"C:\l.log"),
            |e| e != "bcmath" && e != "opcache",
        );
        assert!(ini.contains("extension=intl\n"));
        assert!(!ini.contains("extension=bcmath"));
        assert!(!ini.contains("zend_extension=opcache"));
        assert!(ini.contains("extension_dir=\"C:\\p\\ext\""));
        assert!(ini.contains("curl.cainfo=\"C:\\c.pem\""));
    }

    #[test]
    fn extension_toggle_round_trip() {
        let root = std::env::temp_dir().join(format!("phpev-ext-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let layout = Layout::at(root.clone(), crate::layout::HomeSource::ProcessEnv);
        let dir = version_dir(&layout, "8.4.1");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("php.ini"),
            "[PHP]
extension=intl
",
        )
        .unwrap();

        assert!(enable_extension(&layout, "8.4.1", "soap").unwrap());
        assert!(!enable_extension(&layout, "8.4.1", "soap").unwrap());
        assert!(disable_extension(&layout, "8.4.1", "soap").unwrap());
        assert!(!disable_extension(&layout, "8.4.1", "soap").unwrap());
        assert!(enable_extension(&layout, "8.4.1", "soap").unwrap());
        let ini = std::fs::read_to_string(dir.join("php.ini")).unwrap();
        assert!(ini.contains(";extension=soap") && ini.lines().filter(|l| is_extension_line(l, "soap")).count() == 1);
        assert!(ini.contains("extension=intl"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn php_filter_drops_sdk_and_symbols() {
        assert!(keep_php_entry("php-cgi.exe"));
        assert!(keep_php_entry("extras/ssl/openssl.cnf"));
        assert!(!keep_php_entry("dev/php8.lib"));
        assert!(!keep_php_entry("php8.pdb"));
    }
}
