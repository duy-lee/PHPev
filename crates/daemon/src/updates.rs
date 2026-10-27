//! Looks for a newer PHPev release once a day (unless `[ui] check_updates =
//! false`) and shows a tray balloon once per new version.

use std::sync::Arc;
use std::time::Duration;

use phpev_core::config::Config;
use phpev_core::layout::Layout;
use phpev_core::{clock, update};

use crate::log::log_warn;
use crate::Daemon;

/// First check this long after start, so it never slows the start itself.
const FIRST_DELAY: Duration = Duration::from_secs(60);
/// How often to see whether a check is due (the check itself is daily).
const POLL: Duration = Duration::from_secs(60 * 60);

pub fn spawn(daemon: Arc<Daemon>) {
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_DELAY).await;
        loop {
            check_soon(&daemon.layout);
            tokio::time::sleep(POLL).await;
        }
    });
}

/// Checks in the background if one is due and the check is on.
pub fn check_soon(layout: &Layout) {
    let layout = layout.clone();
    tokio::task::spawn_blocking(move || check_if_due(&layout));
}

fn check_if_due(layout: &Layout) {
    let on = Config::load(&layout.config_file()).is_ok_and(|c| c.ui.check_updates());
    if !on || !update::load(layout).due(clock::now_secs()) {
        return;
    }
    let state = match update::check(layout) {
        Ok(s) => s,
        Err(e) => {
            log_warn!("update check failed: {e}");
            return;
        }
    };
    let Some(version) = state.available() else { return };
    if state.notified.as_deref() == Some(version) {
        return;
    }
    let url = state.url.clone().unwrap_or_default();
    if crate::tray::notify_update(version, &url) {
        let version = version.to_owned();
        if let Err(e) = update::modify(layout, |u| u.notified = Some(version)) {
            log_warn!("could not save state.json: {e}");
        }
    }
}
