//! Newer PHPev releases: one GET to the GitHub releases API a day, nothing
//! about the computer is sent. The answer is kept in `state.json`.

use std::io;

use serde::{Deserialize, Serialize};

use crate::clock;
use crate::layout::Layout;
use crate::net::Http;
use crate::state::State;
use crate::version;

/// Newest first; `/releases/latest` would skip pre-releases, and every 0.x and
/// `-rc` release is published as one.
const RELEASES_URL: &str = "https://api.github.com/repos/duy-lee/PHPev/releases?per_page=30";
/// Seconds between two checks.
pub const INTERVAL: u64 = 24 * 60 * 60;

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// The last answer from GitHub, plus what the user did with it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateState {
    /// Unix time of the last successful check; 0 = never.
    pub checked_at: u64,
    /// Newest published release (`0.2.0`), if any.
    pub latest: Option<String>,
    /// Its release page.
    pub url: Option<String>,
    /// "Skip this version": no banner or menu entry for it.
    pub skipped: Option<String>,
    /// The tray balloon was shown for this version.
    pub notified: Option<String>,
}

impl UpdateState {
    pub fn due(&self, now: u64) -> bool {
        self.checked_at == 0 || now < self.checked_at || now - self.checked_at >= INTERVAL
    }

    /// The release to offer: newer than this build and not skipped.
    pub fn available(&self) -> Option<&str> {
        let latest = self.latest.as_deref()?;
        (is_newer(latest, CURRENT) && self.skipped.as_deref() != Some(latest)).then_some(latest)
    }
}

/// `latest` is a higher version than `current` (both `x.y.z`, a leading `v` is ignored).
pub fn is_newer(latest: &str, current: &str) -> bool {
    let (latest, current) = (latest.trim_start_matches('v'), current.trim_start_matches('v'));
    version::parts(latest).is_some() && version::cmp(latest, current).is_gt()
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Someone on a 0.x or pre-release build (`1.0.0-rc.1`) is told about newer
/// pre-releases too; someone on a stable release only about stable ones.
fn wants_prereleases(current: &str) -> bool {
    current.starts_with("0.") || current.contains('-')
}

/// The newest published release `current` should hear about.
fn newest<'a>(releases: &'a [Release], current: &str) -> Option<&'a Release> {
    let pre = wants_prereleases(current);
    releases
        .iter()
        .filter(|r| !r.draft && (pre || !r.prerelease))
        .filter(|r| version::parts(r.tag_name.trim_start_matches('v')).is_some())
        .max_by(|a, b| version::cmp(a.tag_name.trim_start_matches('v'), b.tag_name.trim_start_matches('v')))
}

/// Asks GitHub for the newest release and saves the answer. Blocking.
pub fn check(layout: &Layout) -> io::Result<UpdateState> {
    let releases: Vec<Release> = Http::new()?.get_json_opt(RELEASES_URL)?.unwrap_or_default();
    let release = newest(&releases, CURRENT);
    modify(layout, |u| {
        u.checked_at = clock::now_secs();
        u.latest = release.map(|r| r.tag_name.trim_start_matches('v').to_owned());
        u.url = release.map(|r| r.html_url.clone());
    })
}

/// Changes the saved update state (skip, notified).
pub fn modify(layout: &Layout, f: impl FnOnce(&mut UpdateState)) -> io::Result<UpdateState> {
    State::modify(layout, |s| {
        f(&mut s.update);
        s.update.clone()
    })
}

/// The saved state, without asking GitHub.
pub fn load(layout: &Layout) -> UpdateState {
    State::load(layout).update
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_versions() {
        assert!(is_newer("0.2.0", "0.1.2"));
        assert!(is_newer("v0.1.10", "0.1.9"));
        assert!(!is_newer("0.1.2", "0.1.2"));
        assert!(!is_newer("0.1.1", "0.1.2"));
        assert!(!is_newer("nightly", "0.1.2"));
    }

    fn release(tag: &str, prerelease: bool, draft: bool) -> Release {
        Release {
            tag_name: tag.into(),
            html_url: format!("https://x/{tag}"),
            draft,
            prerelease,
        }
    }

    #[test]
    fn pre_releases_only_for_pre_release_users() {
        let list = [
            release("v1.0.1-rc.1", true, false),
            release("v1.0.0", false, false),
            release("v1.0.0-rc.2", true, false),
            release("v1.1.0", false, true),
            release("v0.1.3", true, false),
        ];
        assert_eq!(newest(&list, "0.1.2").unwrap().tag_name, "v1.0.1-rc.1");
        assert_eq!(newest(&list, "1.0.0-rc.1").unwrap().tag_name, "v1.0.1-rc.1");
        assert_eq!(newest(&list, "1.0.0").unwrap().tag_name, "v1.0.0");
        assert!(newest(&[release("v1.1.0", false, true)], "1.0.0").is_none());
        assert!(is_newer("1.0.0", "1.0.0-rc.1"));
        assert!(!is_newer("1.0.0-rc.1", "1.0.0"));
    }

    #[test]
    fn due_once_a_day() {
        let s = UpdateState {
            checked_at: 1_000_000,
            ..Default::default()
        };
        assert!(UpdateState::default().due(5));
        assert!(!s.due(1_000_000 + INTERVAL - 1));
        assert!(s.due(1_000_000 + INTERVAL));
        // Clock moved back: check again.
        assert!(s.due(999_000));
    }

    #[test]
    fn skipped_version_is_not_offered() {
        let mut s = UpdateState {
            latest: Some("99.0.0".into()),
            ..Default::default()
        };
        assert_eq!(s.available(), Some("99.0.0"));
        s.skipped = Some("99.0.0".into());
        assert_eq!(s.available(), None);
        s.latest = Some("99.0.1".into());
        assert_eq!(s.available(), Some("99.0.1"));
        s.latest = Some(CURRENT.into());
        assert_eq!(s.available(), None);
    }
}
