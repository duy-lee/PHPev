//! `logs\daemon.log`, rotated by size (5 MB × 3, DESIGN_SPEC §10).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use phpev_core::clock;

const MAX_BYTES: u64 = 5 * 1024 * 1024;
const KEEP: usize = 3;

struct Sink {
    path: PathBuf,
    file: Option<File>,
    size: u64,
}

static SINK: Mutex<Option<Sink>> = Mutex::new(None);

pub fn init(path: &Path) {
    let file = OpenOptions::new().create(true).append(true).open(path).ok();
    let size = file.as_ref().and_then(|f| f.metadata().ok()).map_or(0, |m| m.len());
    *SINK.lock().unwrap_or_else(|e| e.into_inner()) = Some(Sink {
        path: path.to_owned(),
        file,
        size,
    });
}

pub fn write(level: &str, msg: &str) {
    let mut guard = SINK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(sink) = guard.as_mut() else { return };
    if sink.size > MAX_BYTES {
        rotate(sink);
    }
    let line = format!("{} {level:<5} {msg}\n", clock::timestamp());
    if let Some(f) = sink.file.as_mut() {
        if f.write_all(line.as_bytes()).is_ok() {
            sink.size += line.len() as u64;
        }
    }
}

/// daemon.log → .1 → .2 → .3 (dropped)
fn rotate(sink: &mut Sink) {
    sink.file = None;
    let numbered = |n: usize| sink.path.with_extension(format!("log.{n}"));
    let _ = std::fs::remove_file(numbered(KEEP));
    for n in (1..KEEP).rev() {
        let _ = std::fs::rename(numbered(n), numbered(n + 1));
    }
    let _ = std::fs::rename(&sink.path, numbered(1));
    sink.file = OpenOptions::new().create(true).append(true).open(&sink.path).ok();
    sink.size = 0;
}

macro_rules! log_info {
    ($($arg:tt)*) => { $crate::log::write("INFO", &format!($($arg)*)) };
}
macro_rules! log_warn {
    ($($arg:tt)*) => { $crate::log::write("WARN", &format!($($arg)*)) };
}
pub(crate) use {log_info, log_warn};
