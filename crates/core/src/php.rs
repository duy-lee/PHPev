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
const ARCHIVE_BASE: &str = "https://downloads.php.net/~windows/releases/archives/";

/// End-of-life lines that left releases.json: the final release of each, from
/// the archive. The archive publishes no checksums, so these are pinned here
/// (identical on downloads.php.net and windows.php.net when recorded).
const ARCHIVED: &[(&str, &str, &str, &str, &str)] = &[
    // (line, version, zip, sha256, toolset)
    (
        "7.3",
        "7.3.33",
        "php-7.3.33-nts-Win32-VC15-x64.zip",
        "5eaf3cad80e678623f222a42c99bcefcc60eea359d407fb51e805afdb3b13e5e",
        "vc15",
    ),
    (
        "7.2",
        "7.2.34",
        "php-7.2.34-nts-Win32-VC15-x64.zip",
        "3c673eab656e26fd6bc3ad27fe71169ad888b04e21d63d3c6b3151d5ed216563",
        "vc15",
    ),
    (
        "7.1",
        "7.1.33",
        "php-7.1.33-nts-Win32-VC14-x64.zip",
        "071438be5bbaee8b34894ddba7852b6040991a3c853ab8141efcb1b6655bbbef",
        "vc14",
    ),
    (
        "7.0",
        "7.0.33",
        "php-7.0.33-nts-Win32-VC14-x64.zip",
        "bba39bde5b0bd50efadc11e2716c7528669945f9d1397d707486e401e67e89fb",
        "vc14",
    ),
];
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
    /// Visual C++ toolset the build needs (`vc14`, `vc15`, `vs16`, `vs17`).
    pub toolset: String,
    /// Downloaded from the archive of end-of-life releases.
    pub archived: bool,
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
            version: without_rebuild(version).to_owned(),
            zip: build.zip.path,
            sha256: build.zip.sha256,
            toolset: key.trim_start_matches("nts-").trim_end_matches("-x64").to_owned(),
            archived: false,
        });
    }
    out.sort_by(|a, b| version::cmp(&b.version, &a.version));
    Ok(out)
}

/// `8.4.27-1` → `8.4.27`: windows.php.net numbers rebuilds of the same PHP
/// release; the PHP version (what `php -v` reports and the install folder) has
/// no suffix. Only the zip name keeps it.
fn without_rebuild(version: &str) -> &str {
    match version.rsplit_once('-') {
        Some((base, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => base,
        _ => version,
    }
}

/// Current builds from releases.json, then the archived lines it no longer lists.
pub fn available(http: &Http) -> io::Result<Vec<Release>> {
    let mut out = parse_releases(&http.get_text(RELEASES_URL)?)?;
    add_archived(&mut out);
    Ok(out)
}

fn add_archived(releases: &mut Vec<Release>) {
    for &(line, version, zip, sha256, toolset) in ARCHIVED {
        if !releases.iter().any(|r| r.line == line) {
            releases.push(Release {
                line: line.to_owned(),
                version: version.to_owned(),
                zip: zip.to_owned(),
                sha256: sha256.to_owned(),
                toolset: toolset.to_owned(),
                archived: true,
            });
        }
    }
    releases.sort_by(|a, b| version::cmp(&b.version, &a.version));
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
    let base = if release.archived { ARCHIVE_BASE } else { DOWNLOAD_BASE };
    let url = format!("{base}{}", release.zip);
    net::fetch_verified(http, &url, &zip, &release.sha256, progress)?;

    let mut stats = Stats::default();
    extract::install_atomically(&dir, |tmp| {
        stats = extract::extract_zip(&zip, tmp, false, &keep_php_entry)?;
        Ok(())
    })?;
    std::fs::remove_file(&zip)?;
    Ok(Some(stats))
}

/// A PHP build found in a folder the user wants to add.
#[derive(Debug)]
pub struct FolderBuild {
    /// e.g. `7.1.33` (a suffix such as `RC1` is dropped).
    pub version: String,
    /// Thread-safe (TS) build; PHPev's own downloads are NTS.
    pub thread_safe: bool,
}

/// Checks that `dir` holds a 64-bit PHP build PHPev can run (`php.exe` for the
/// CLI, `php-cgi.exe` for sites) and asks it for its version.
pub fn inspect_folder(dir: &Path) -> io::Result<FolderBuild> {
    let invalid = |msg: String| io::Error::new(io::ErrorKind::InvalidInput, msg);
    for exe in ["php.exe", "php-cgi.exe"] {
        if !dir.join(exe).is_file() {
            return Err(invalid(format!(
                "{} has no {exe}; choose the folder that contains php.exe",
                dir.display()
            )));
        }
    }
    if !is_x64_exe(&dir.join("php.exe"))? {
        return Err(invalid(format!(
            "{} is a 32-bit PHP; PHPev runs 64-bit (x64) builds only",
            dir.display()
        )));
    }
    // -n: the folder's own php.ini may point at paths that no longer exist.
    let out = std::process::Command::new(dir.join("php.exe"))
        .args(["-n", "-r", "echo PHP_VERSION, '|', PHP_ZTS;"])
        .output()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (raw, zts) = text
        .trim()
        .split_once('|')
        .filter(|_| out.status.success())
        .ok_or_else(|| {
            io::Error::other(format!(
                "{} did not start ({}). It may need a Visual C++ runtime this computer does not have.",
                dir.join("php.exe").display(),
                out.status
            ))
        })?;
    let version: String = raw.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let version = version.trim_end_matches('.').to_owned();
    if version::parts(&version).is_none_or(|p| p.len() != 3) {
        return Err(io::Error::other(format!("unexpected PHP version `{raw}`")));
    }
    Ok(FolderBuild {
        version,
        thread_safe: zts.trim() == "1",
    })
}

/// True if `exe` is a PE image for x64 (IMAGE_FILE_MACHINE_AMD64).
fn is_x64_exe(exe: &Path) -> io::Result<bool> {
    let bytes = std::fs::read(exe)?;
    let pe = bytes
        .get(0x3C..0x40)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    Ok(pe
        .and_then(|at| bytes.get(at..at + 6))
        .is_some_and(|h| &h[..4] == b"PE\0\0" && h[4..6] == [0x64, 0x86]))
}

/// Copies a PHP build into the PHPev home as `version`, leaving out what
/// `install` leaves out of downloads. Its php.ini is kept as `php.ini.orig`,
/// so PHPev can write one with paths that work here. Returns the files copied.
pub fn copy_folder(layout: &Layout, src: &Path, version: &str) -> io::Result<usize> {
    let dir = version_dir(layout, version);
    if dir.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("PHP {version} is already installed ({})", dir.display()),
        ));
    }
    let mut files = 0;
    extract::install_atomically(&dir, |tmp| {
        copy_tree(src, tmp, "", &mut files)?;
        let ini = tmp.join("php.ini");
        if ini.is_file() {
            std::fs::rename(&ini, tmp.join("php.ini.orig"))?;
        }
        Ok(())
    })?;
    Ok(files)
}

fn copy_tree(src: &Path, dest: &Path, rel: &str, files: &mut usize) -> io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if keep_php_entry(&format!("{rel}/")) {
                copy_tree(&entry.path(), &dest.join(&name), &rel, files)?;
            }
        } else if kind.is_file() && keep_php_entry(&rel) {
            std::fs::copy(entry.path(), dest.join(&name))?;
            *files += 1;
        }
    }
    Ok(())
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
        short_extension_names(version),
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
    text.push_str(&format!(
        "; enabled by PHPev\nextension={}\n",
        extension_name(version, ext)
    ));
    std::fs::write(ini, text)?;
    Ok(true)
}

/// `extension=intl` works from PHP 7.2; older versions need `php_intl.dll`.
/// Unparsable versions are assumed new.
pub fn short_extension_names(version: &str) -> bool {
    version::parts(version).is_none_or(|p| p.as_slice() >= [7, 2].as_slice())
}

/// The value of an `extension=` line for `ext` in this version's php.ini.
fn extension_name(version: &str, ext: &str) -> String {
    if short_extension_names(version) {
        ext.to_owned()
    } else {
        format!("php_{ext}.dll")
    }
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

fn render_php_ini(
    ext_dir: &Path,
    cacert: &Path,
    error_log: &Path,
    short_names: bool,
    has_ext: impl Fn(&str) -> bool,
) -> String {
    let dll = |ext: &str| {
        if short_names {
            ext.to_owned()
        } else {
            format!("php_{ext}.dll")
        }
    };
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
        s.push_str(&format!("extension={}\n", dll(ext)));
    }
    s.push_str("\n[curl]\n");
    s.push_str(&format!("curl.cainfo=\"{}\"\n", cacert.display()));
    s.push_str("\n[openssl]\n");
    s.push_str(&format!("openssl.cafile=\"{}\"\n", cacert.display()));

    s.push_str("\n[opcache]\n");
    // From PHP 8.5 OPcache is built in and no longer a loadable DLL.
    if has_ext("opcache") {
        s.push_str(&format!("zend_extension={}\n", dll("opcache")));
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
        "vc14" | "vc15" => (14, 0),
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
    vc_runtime_version().is_some_and(|v| vc_runtime_ok_for(v, toolset))
}

fn vc_runtime_ok_for(installed: (u32, u32), toolset: &str) -> bool {
    installed >= required_vc_runtime(toolset)
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
    fn rebuild_suffix_is_not_part_of_the_version() {
        let json = r#"{
            "8.2": {"version": "8.2.35-1",
                    "nts-vs16-x64": {"zip": {"path": "php-8.2.35-1-nts-Win32-vs16-x64.zip", "size": "1", "sha256": "aa"}}},
            "8.4": {"version": "8.4.27-1",
                    "nts-vs17-x64": {"zip": {"path": "php-8.4.27-1-nts-Win32-vs17-x64.zip", "size": "1", "sha256": "bb"}}},
            "8.1": {"version": "8.1.34",
                    "nts-vs16-x64": {"zip": {"path": "php-8.1.34-nts-Win32-vs16-x64.zip", "size": "1", "sha256": "cc"}}}
        }"#;
        let r = parse_releases(json).unwrap();
        let versions: Vec<&str> = r.iter().map(|r| r.version.as_str()).collect();
        assert_eq!(versions, ["8.4.27", "8.2.35", "8.1.34"]);
        assert_eq!(r[0].zip, "php-8.4.27-1-nts-Win32-vs17-x64.zip");
        assert_eq!(resolve(&r, "8.4").unwrap().version, "8.4.27");
        assert_eq!(without_rebuild("8.5.0-RC1"), "8.5.0-RC1");
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
            true,
            |e| e != "bcmath" && e != "opcache",
        );
        assert!(ini.contains("extension=intl\n"));
        assert!(!ini.contains("extension=bcmath"));
        assert!(!ini.contains("zend_extension=opcache"));
        assert!(ini.contains("extension_dir=\"C:\\p\\ext\""));
        assert!(ini.contains("curl.cainfo=\"C:\\c.pem\""));
    }

    #[test]
    fn ini_before_php_7_2_uses_dll_names() {
        assert!(!short_extension_names("7.1.33"));
        assert!(short_extension_names("7.2.0"));
        assert!(short_extension_names("8.4.27"));
        let ini = render_php_ini(
            Path::new(r"C:\p\ext"),
            Path::new(r"C:\c.pem"),
            Path::new(r"C:\l.log"),
            false,
            |_| true,
        );
        assert!(ini.contains("extension=php_intl.dll\n"));
        assert!(ini.contains("zend_extension=php_opcache.dll\n"));
        assert!(!ini.contains("extension=intl\n"));
    }

    #[test]
    fn archived_lines_fill_in_what_releases_json_dropped() {
        let mut r = parse_releases(SAMPLE).unwrap();
        add_archived(&mut r);
        let r71 = resolve(&r, "7.1").unwrap();
        assert!(r71.archived);
        assert_eq!(r71.version, "7.1.33");
        assert_eq!(r71.toolset, "vc14");
        assert!(!resolve(&r, "8.4").unwrap().archived);
        assert!(r.windows(2).all(|w| version::cmp(&w[0].version, &w[1].version).is_ge()));
        assert!(vc_runtime_ok_for((14, 0), "vc14") && !vc_runtime_ok_for((14, 20), "vs17"));
    }

    #[test]
    fn legacy_extension_toggle_writes_dll_name() {
        let root = std::env::temp_dir().join(format!("phpev-ext71-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let layout = Layout::at(root.clone(), crate::layout::HomeSource::ProcessEnv);
        let dir = version_dir(&layout, "7.1.33");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("php.ini"), "[PHP]\nextension=php_intl.dll\n").unwrap();
        assert!(enable_extension(&layout, "7.1.33", "soap").unwrap());
        assert!(!enable_extension(&layout, "7.1.33", "intl").unwrap());
        let ini = std::fs::read_to_string(dir.join("php.ini")).unwrap();
        assert!(ini.contains("extension=php_soap.dll\n"));
        assert!(disable_extension(&layout, "7.1.33", "soap").unwrap());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn copies_a_folder_without_sdk_and_keeps_its_ini_aside() {
        let root = std::env::temp_dir().join(format!("phpev-add-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let src = root.join("php71");
        std::fs::create_dir_all(src.join("ext")).unwrap();
        std::fs::create_dir_all(src.join("dev")).unwrap();
        for f in [
            "php.exe",
            "php-cgi.exe",
            "php.ini",
            "php7.pdb",
            "ext/php_intl.dll",
            "dev/php7.lib",
        ] {
            std::fs::write(src.join(f), "x").unwrap();
        }
        let layout = Layout::at(root.join("home"), crate::layout::HomeSource::ProcessEnv);
        assert_eq!(copy_folder(&layout, &src, "7.1.33").unwrap(), 4);
        let dir = version_dir(&layout, "7.1.33");
        assert!(dir.join("ext/php_intl.dll").is_file() && dir.join("php.ini.orig").is_file());
        assert!(!dir.join("php.ini").exists() && !dir.join("dev").exists() && !dir.join("php7.pdb").exists());
        assert!(src.join("php.ini").is_file(), "the source folder is left alone");
        assert_eq!(
            copy_folder(&layout, &src, "7.1.33").unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert!(inspect_folder(&src).is_err(), "a fake php.exe is not an x64 image");
        std::fs::remove_dir_all(&root).unwrap();
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
