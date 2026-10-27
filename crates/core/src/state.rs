//! Runtime state the daemon restores on start (`state.json`, daemon-written).

use std::io;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::config::write_atomic;
use crate::layout::Layout;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// MariaDB was running when the daemon last stopped (or the user ran `db start`).
    pub mariadb_running: bool,
    /// Newer-release check (see `update`).
    #[serde(skip_serializing_if = "is_default")]
    pub update: crate::update::UpdateState,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
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

    /// Load, change, save, with one writer at a time in this process.
    pub fn modify<R>(layout: &Layout, f: impl FnOnce(&mut State) -> R) -> io::Result<R> {
        static LOCK: Mutex<()> = Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut state = State::load(layout);
        let result = f(&mut state);
        state.save(layout)?;
        Ok(result)
    }
}
