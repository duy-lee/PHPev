//! Installing composer.phar from getcomposer.org.

use std::io;
use std::path::PathBuf;

use crate::layout::Layout;
use crate::net::{self, Http, Progress};

const PHAR_URL: &str = "https://getcomposer.org/download/latest-stable/composer.phar";

pub fn phar_path(layout: &Layout) -> PathBuf {
    layout.composer_dir().join("composer.phar")
}

pub fn is_installed(layout: &Layout) -> bool {
    phar_path(layout).is_file()
}

/// Downloads the latest stable composer.phar, replacing any existing one only
/// after the new file is verified.
pub fn install_latest(http: &Http, layout: &Layout, progress: Progress<'_>) -> io::Result<()> {
    let sha = http.get_text(&format!("{PHAR_URL}.sha256sum"))?;
    let sha = net::parse_checksum_file(&sha)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unexpected composer.phar.sha256sum format"))?
        .to_owned();
    std::fs::create_dir_all(layout.composer_dir())?;
    // `download` writes to a `.part` file and renames on success, so a failed
    // update keeps the old phar.
    http.download(PHAR_URL, &phar_path(layout), Some(&sha), progress)
}
