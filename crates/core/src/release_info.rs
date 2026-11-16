//! What PHPev learned about a downloaded PHP, MariaDB or Composer release when it
//! installed it, saved next to the files (`phpev-release.json`) so it can be
//! shown later without asking the internet again. Releases installed before
//! this file existed, or added from a folder, have none.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const FILE: &str = "phpev-release.json";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseInfo {
    /// Exact version, e.g. `11.4.8`.
    pub version: String,
    /// UTC date of the install, `YYYY-MM-DD`.
    pub installed: String,
    /// Where it was downloaded from.
    pub url: String,
    /// SHA-256 the download was checked against.
    pub sha256: String,
    /// A long-term support release (MariaDB, Composer 2.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lts: Option<bool>,
    /// End of support as announced when installed, `YYYY-MM-DD` (MariaDB).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eol: Option<String>,
    /// Visual C++ toolset the build needs (PHP).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolset: Option<String>,
    /// Came from the archive of end-of-life releases (PHP).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
    /// The oldest PHP it runs on (Composer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_php: Option<String>,
}

/// Writes `info` into the release folder `dir`.
pub fn save(dir: &Path, info: &ReleaseInfo) -> io::Result<()> {
    let json = serde_json::to_string_pretty(info).map_err(io::Error::other)?;
    std::fs::write(dir.join(FILE), json)
}

/// The saved facts of the release in `dir`, if any (a damaged file counts as none).
pub fn load(dir: &Path) -> Option<ReleaseInfo> {
    serde_json::from_slice(&std::fs::read(dir.join(FILE)).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_loads_without_empty_fields() {
        let dir = std::env::temp_dir().join(format!("phpev-release-info-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let info = ReleaseInfo {
            version: "11.4.8".into(),
            installed: "2026-11-16".into(),
            url: "https://example.org/mariadb.zip".into(),
            sha256: "ab".repeat(32),
            lts: Some(true),
            eol: Some("2029-05-29".into()),
            ..Default::default()
        };
        save(&dir, &info).unwrap();
        let text = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(!text.contains("toolset") && !text.contains("min_php"));
        assert_eq!(load(&dir), Some(info));
        std::fs::write(dir.join(FILE), "not json").unwrap();
        assert_eq!(load(&dir), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
