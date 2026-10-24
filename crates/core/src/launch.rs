//! Opening things for the user (browser, app window, Explorer, terminal), from
//! the CLI, the dashboard API and the tray.
//!
//! These are started with CREATE_BREAKAWAY_FROM_JOB so they live outside the
//! daemon's kill-on-close job: closing PHPev must not close the user's windows.

use std::io;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::registry::{self, Hive};

const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ERROR_ACCESS_DENIED: i32 = 5;

fn spawn(cmd: &mut Command, flags: u32) -> io::Result<()> {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    match cmd.creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB).spawn() {
        // A terminal's job that does not allow breakaway (CLI only; the
        // daemon's own job allows it): start inside it.
        Err(e) if e.raw_os_error() == Some(ERROR_ACCESS_DENIED) => cmd.creation_flags(flags).spawn().map(|_| ()),
        other => other.map(|_| ()),
    }
}

fn check_http(url: &str) -> io::Result<()> {
    if url.starts_with("http://") || url.starts_with("https://") {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "only http(s) URLs can be opened",
        ))
    }
}

/// Opens `url` (http/https only) in the default browser.
pub fn open_url(url: &str) -> io::Result<()> {
    check_http(url)?;
    // url.dll hands the URL to the registered handler without a shell parsing it.
    spawn(
        Command::new(system32("rundll32.exe")).args(["url.dll,FileProtocolHandler", url]),
        CREATE_NO_WINDOW,
    )
}

/// Opens `url` in its own app-style window (`msedge --app`),
/// or in the default browser when Edge is missing. Returns whether Edge was used.
pub fn open_app_window(url: &str) -> io::Result<bool> {
    check_http(url)?;
    match edge_exe() {
        Some(edge) => spawn(Command::new(edge).arg(format!("--app={url}")), 0).map(|()| true),
        None => open_url(url).map(|()| false),
    }
}

/// Microsoft Edge, which ships with Windows 10/11 (but can be removed in some regions).
pub fn edge_exe() -> Option<PathBuf> {
    const APP_PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\msedge.exe";
    let registered = [Hive::CurrentUser, Hive::LocalMachine]
        .into_iter()
        .find_map(|hive| registry::read_string(hive, APP_PATH, "").ok().flatten())
        .map(|p| PathBuf::from(p.trim_matches('"')));
    registered
        .into_iter()
        .chain(
            ["ProgramFiles(x86)", "ProgramFiles"]
                .into_iter()
                .filter_map(std::env::var_os)
                .map(|dir| PathBuf::from(dir).join(r"Microsoft\Edge\Application\msedge.exe")),
        )
        .find(|p| p.is_file())
}

pub fn open_folder(dir: &Path) -> io::Result<()> {
    spawn(Command::new(windows_dir().join("explorer.exe")).arg(dir), 0)
}

/// Windows Terminal in `dir` if installed, otherwise a console window.
pub fn open_terminal(dir: &Path) -> io::Result<()> {
    let wt = std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join(r"Microsoft\WindowsApps\wt.exe"));
    match wt.filter(|p| p.is_file()) {
        Some(wt) => spawn(Command::new(wt).arg("-d").arg(dir), 0),
        None => spawn(
            Command::new(system32("cmd.exe")).arg("/k").current_dir(dir),
            CREATE_NEW_CONSOLE,
        ),
    }
}

fn windows_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into()))
}

fn system32(exe: &str) -> PathBuf {
    windows_dir().join("System32").join(exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_non_http() {
        assert!(open_url("file:///C:/Windows").is_err());
        assert!(open_app_window("javascript:alert(1)").is_err());
    }

    #[test]
    fn edge_is_found_if_installed() {
        // On the dev machine Edge exists; elsewhere this just must not panic.
        if let Some(p) = edge_exe() {
            assert!(p.ends_with("msedge.exe"), "{}", p.display());
        }
    }
}
