//! Installing Composer from getcomposer.org.
//!
//! Every Composer is a version folder, `<composer dir>\<version>\composer.phar`,
//! shared by the sites that run it. A site runs the version picked for it
//! (`phpev composer pin`), else the default (`phpev composer use`), else the
//! newest installed. A version its PHP cannot run (Composer 2.3+ needs PHP
//! 7.2.5) is refused, never swapped for another.

use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::layout::Layout;
use crate::net::{self, Http, Progress};
use crate::version;

/// Every tagged Composer release.
const RELEASES_URL: &str = "https://repo.packagist.org/p2/composer/composer.json";

pub const PHAR: &str = "composer.phar";
/// The phars PHPev kept next to the version folders before 1.0: the latest
/// Composer and the 2.2 LTS one. `ops::adopt_legacy_composer` turns them into
/// versions.
pub const LEGACY_PHARS: [&str; 2] = ["composer.phar", "composer-2.2.phar"];
/// The oldest PHP Composer 2.3 and later run on.
const LATEST_MIN_PHP: &str = "7.2.5";
/// The oldest PHP Composer 2.2 runs on.
const LTS_MIN_PHP: &str = "5.3.2";
/// Older Composer releases are not offered.
const OLDEST_LINE: &str = "2.2";

/// The oldest PHP Composer `version` runs on.
pub fn min_php(version: &str) -> &'static str {
    if version::cmp(version, "2.3").is_lt() {
        LTS_MIN_PHP
    } else {
        LATEST_MIN_PHP
    }
}

/// True if Composer `version` runs on PHP `php_version`.
pub fn runs_on(version: &str, php_version: &str) -> bool {
    version::cmp(php_version, min_php(version)).is_ge()
}

/// Folder of Composer `version`.
pub fn version_dir(layout: &Layout, version: &str) -> PathBuf {
    layout.composer_dir().join(version)
}

/// Composer versions installed in the Composer folder `dir`, newest first.
pub fn installed(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| version::parts(name).is_some() && dir.join(name).join(PHAR).is_file())
        .collect();
    v.sort_by(|a, b| version::cmp(b, a));
    v
}

/// Why a Composer runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// Picked for the site.
    Site,
    /// The default.
    Default,
    /// No default: the newest installed.
    Newest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picked {
    pub version: String,
    pub phar: PathBuf,
    pub why: Why,
}

/// The Composer the `composer` shim runs with PHP `php_version`, in the
/// Composer folder `dir`: the version `pinned` for the site (`2.8` or
/// `2.8.12`), else `default`, else the newest installed.
pub fn pick(dir: &Path, php_version: &str, pinned: Option<&str>, default: Option<&str>) -> Result<Picked, String> {
    let installed = installed(dir);
    let (version, why) = match (pinned, default) {
        (Some(wanted), _) => {
            let found = version::best_match(wanted, installed.iter().map(String::as_str)).ok_or_else(|| {
                format!(
                    "Composer {wanted} (picked for this site) is not installed. Run: phpev composer install {wanted}"
                )
            })?;
            (found, Why::Site)
        }
        (None, Some(wanted)) => {
            let found = version::best_match(wanted, installed.iter().map(String::as_str)).ok_or_else(|| {
                format!("Composer {wanted} (the default) is not installed. Run: phpev composer install {wanted}")
            })?;
            (found, Why::Default)
        }
        (None, None) => (
            installed
                .first()
                .map(String::as_str)
                .ok_or("Composer is not installed. Run: phpev composer install")?,
            Why::Newest,
        ),
    };
    if !runs_on(version, php_version) {
        let need = min_php(version);
        return Err(match why {
            Why::Site => format!(
                "Composer {version} needs PHP {need} or later, but this site runs PHP {php_version}. Pick another one: phpev composer pin <version>"
            ),
            _ => format!(
                "Composer {version} (the default) needs PHP {need} or later, but PHP {php_version} runs here. Pick a Composer for this site (phpev composer pin 2.2) or change the default (phpev composer use <version>)"
            ),
        });
    }
    Ok(Picked {
        phar: dir.join(version).join(PHAR),
        version: version.to_owned(),
        why,
    })
}

/// The version written in a composer.phar (`const VERSION = '2.8.12'`), read
/// without running PHP.
pub fn phar_version(phar: &Path) -> Option<String> {
    const NEEDLE: &[u8] = b"const VERSION = '";
    let bytes = std::fs::read(phar).ok()?;
    let at = bytes.windows(NEEDLE.len()).position(|w| w == NEEDLE)? + NEEDLE.len();
    let end = bytes[at..].iter().position(|&b| b == b'\'')?;
    let v = std::str::from_utf8(&bytes[at..at + end]).ok()?;
    is_stable(v).then(|| v.to_owned())
}

/// Downloads Composer `version` (exact, e.g. `2.8.12`).
pub fn install_version(http: &Http, layout: &Layout, version: &str, progress: Progress<'_>) -> io::Result<()> {
    let url = format!("https://getcomposer.org/download/{version}/composer.phar");
    download(http, &url, &version_dir(layout, version).join(PHAR), progress)
}

#[derive(Deserialize)]
struct Releases {
    packages: std::collections::HashMap<String, Vec<Release>>,
}

#[derive(Deserialize)]
struct Release {
    version: String,
}

/// Stable Composer releases from 2.2 on, newest first.
pub fn released(http: &Http) -> io::Result<Vec<String>> {
    let releases: Releases = http.get_json(RELEASES_URL)?;
    let mut v: Vec<String> = releases
        .packages
        .into_values()
        .flatten()
        .map(|r| r.version)
        .filter(|v| is_stable(v) && version::cmp(v, OLDEST_LINE).is_ge())
        .collect();
    v.sort_by(|a, b| version::cmp(b, a));
    v.dedup();
    Ok(v)
}

/// `2.8.12`, not `2.9.0-RC1`.
fn is_stable(v: &str) -> bool {
    !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit() || b == b'.')
}

/// The newest release of each minor line (`2.11.0`, `2.10.3`, …), newest first.
pub fn lines(released: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for v in released {
        if !out.iter().any(|o| line_of(o) == line_of(v)) {
            out.push(v.clone());
        }
    }
    out
}

/// `2.8.12` → `2.8`.
pub fn line_of(v: &str) -> String {
    v.split('.').take(2).collect::<Vec<_>>().join(".")
}

fn download(http: &Http, url: &str, dest: &Path, progress: Progress<'_>) -> io::Result<()> {
    let sha = http.get_text(&format!("{url}.sha256sum"))?;
    let sha = net::parse_checksum_file(&sha)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unexpected composer.phar.sha256sum format"))?
        .to_owned();
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // `download` writes to a `.part` file and renames on success, so a failed
    // download leaves nothing behind.
    http.download(url, dest, Some(&sha), progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composer_2_3_and_later_need_php_7_2_5() {
        assert!(runs_on("2.2.31", "5.6.40"));
        assert!(runs_on("2.2.31", "8.4.27"));
        assert!(!runs_on("2.3.0", "7.1.33"));
        assert!(runs_on("2.8.12", "7.2.5"));
        assert!(!runs_on("2.11.0", "7.2.4"));
    }

    #[test]
    fn stable_lines() {
        let released: Vec<String> = ["2.11.0", "2.10.3", "2.10.2", "2.9.8", "2.2.31", "2.2.30"]
            .map(String::from)
            .to_vec();
        assert_eq!(lines(&released), ["2.11.0", "2.10.3", "2.9.8", "2.2.31"]);
        assert!(is_stable("2.10.3"));
        assert!(!is_stable("2.10.0-RC1"));
        assert_eq!(line_of("2.10.3"), "2.10");
    }

    #[test]
    fn site_then_default_then_newest() {
        let dir = std::env::temp_dir().join(format!("phpev-composer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(pick(&dir, "8.4.27", None, None).unwrap_err().contains("not installed"));
        for v in ["2.8.12", "2.8.3", "2.2.31"] {
            std::fs::create_dir_all(dir.join(v)).unwrap();
            std::fs::write(dir.join(v).join(PHAR), "").unwrap();
        }
        assert_eq!(installed(&dir), ["2.8.12", "2.8.3", "2.2.31"]);
        let picked = |php, pinned, default| pick(&dir, php, pinned, default).map(|p| (p.version, p.why));

        assert_eq!(picked("8.4.27", None, None).unwrap(), ("2.8.12".into(), Why::Newest));
        assert_eq!(
            picked("8.4.27", None, Some("2.8.3")).unwrap(),
            ("2.8.3".into(), Why::Default)
        );
        assert_eq!(
            picked("7.4.33", Some("2.8"), Some("2.2")).unwrap(),
            ("2.8.12".into(), Why::Site)
        );
        assert_eq!(
            picked("7.1.33", Some("2.2"), None).unwrap(),
            ("2.2.31".into(), Why::Site)
        );
        assert_eq!(
            pick(&dir, "7.4.33", Some("2.8.3"), None).unwrap().phar,
            dir.join(r"2.8.3\composer.phar")
        );
        // Too new for the PHP: refused, not swapped for the 2.2 that would run.
        assert!(picked("7.1.33", Some("2.8"), None)
            .unwrap_err()
            .contains("needs PHP 7.2.5"));
        assert!(picked("7.1.33", None, Some("2.8")).unwrap_err().contains("the default"));
        assert!(picked("7.1.33", None, None).unwrap_err().contains("needs PHP 7.2.5"));
        assert!(picked("8.4.27", Some("2.5"), None)
            .unwrap_err()
            .contains("not installed"));
        assert!(picked("8.4.27", None, Some("2.5")).unwrap_err().contains("the default"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn version_written_in_the_phar() {
        let dir = std::env::temp_dir().join(format!("phpev-composer-phar-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let phar = dir.join(PHAR);
        std::fs::write(
            &phar,
            "<?php class Composer { public const VERSION = '2.8.12'; const RELEASE_DATE = 'x'; }",
        )
        .unwrap();
        assert_eq!(phar_version(&phar).as_deref(), Some("2.8.12"));
        std::fs::write(&phar, "const VERSION = '@package_version@';").unwrap();
        assert_eq!(phar_version(&phar), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
