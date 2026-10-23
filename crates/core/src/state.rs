//! Runtime state the daemon restores on start (`state.json`, daemon-written).

use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::write_atomic;
use crate::layout::Layout;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// MariaDB was running when the daemon last stopped (or the user ran `db start`).
    pub mariadb_running: bool,
}

fn path(layout: &Layout) -> PathBuf {
    layout.root().join("state.json")
}

impl State {
    pub fn load(layout: &Layout) -> Self {
        std::fs::read_to_string(path(layout))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, layout: &Layout) -> io::Result<()> {
        let json = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        write_atomic(&path(layout), &json)
    }
}
