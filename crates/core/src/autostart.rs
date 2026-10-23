//! Starting the daemon at log in (HKCU Run key), shared by `phpev autostart`
//! and the tray menu.

use std::io;

use crate::layout::Layout;
use crate::registry::{self, Hive};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "PHPev";
const DAEMON_EXE: &str = "phpev-daemon.exe";

pub fn is_enabled() -> io::Result<bool> {
    Ok(registry::read_string(Hive::CurrentUser, RUN_KEY, RUN_VALUE)?.is_some())
}

pub fn set(layout: &Layout, on: bool) -> io::Result<()> {
    if !on {
        return registry::delete_value(Hive::CurrentUser, RUN_KEY, RUN_VALUE);
    }
    // Point at the installed copy, not wherever this binary was built.
    let exe = layout.shims().join(DAEMON_EXE);
    if !exe.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "run `phpev setup` first"));
    }
    registry::write_string(Hive::CurrentUser, RUN_KEY, RUN_VALUE, &format!("\"{}\"", exe.display()))
}
