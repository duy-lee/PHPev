//! Installing composer.phar from getcomposer.org.
//!
//! Composer 2.3 and later need PHP 7.2.5, so PHPev also keeps the 2.2 LTS
//! phar next to the latest one; the shim picks the phar the PHP can run.

use std::io;
use std::path::{Path, PathBuf};

use crate::layout::Layout;
use crate::net::{self, Http, Progress};
use crate::version;

const PHAR_URL: &str = "https://getcomposer.org/download/latest-stable/composer.phar";
const LTS_URL: &str = "https://getcomposer.org/download/latest-2.2.x/composer.phar";

pub const PHAR: &str = "composer.phar";
pub const LTS_PHAR: &str = "composer-2.2.phar";
/// The oldest PHP the latest Composer runs on.
const LATEST_MIN_PHP: &str = "7.2.5";

pub fn phar_path(layout: &Layout) -> PathBuf {
    layout.composer_dir().join(PHAR)
}

pub fn lts_path(layout: &Layout) -> PathBuf {
    layout.composer_dir().join(LTS_PHAR)
}

pub fn is_installed(layout: &Layout) -> bool {
    phar_path(layout).is_file()
}

/// True if an installed PHP is too old for the latest Composer.
pub fn lts_wanted(layout: &Layout) -> bool {
    crate::php::installed(layout).iter().any(|v| needs_lts(v))
}

/// True if PHP `php_version` is too old for the latest Composer.
pub fn needs_lts(php_version: &str) -> bool {
    version::cmp(php_version, LATEST_MIN_PHP).is_lt()
}

/// The phar to run with PHP `php_version`, in the Composer folder `dir`.
pub fn phar_for(dir: &Path, php_version: &str) -> PathBuf {
    dir.join(if needs_lts(php_version) { LTS_PHAR } else { PHAR })
}

/// Downloads the latest stable composer.phar, replacing any existing one only
/// after the new file is verified.
pub fn install_latest(http: &Http, layout: &Layout, progress: Progress<'_>) -> io::Result<()> {
    download(http, PHAR_URL, &phar_path(layout), progress)
}

/// Downloads the latest Composer 2.2 LTS, for PHP older than 7.2.5.
pub fn install_lts(http: &Http, layout: &Layout, progress: Progress<'_>) -> io::Result<()> {
    download(http, LTS_URL, &lts_path(layout), progress)
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
    // update keeps the old phar.
    http.download(url, dest, Some(&sha), progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn php_before_7_2_5_gets_the_lts_phar() {
        let dir = Path::new(r"C:\h\bin\composer");
        assert_eq!(phar_for(dir, "7.1.33"), dir.join(LTS_PHAR));
        assert_eq!(phar_for(dir, "7.2.4"), dir.join(LTS_PHAR));
        assert_eq!(phar_for(dir, "7.2.5"), dir.join(PHAR));
        assert_eq!(phar_for(dir, "8.4.27"), dir.join(PHAR));
    }
}
