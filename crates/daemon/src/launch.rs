//! Opening things for the user (browser, Explorer, terminal) from the daemon.
//!
//! These are started with CREATE_BREAKAWAY_FROM_JOB so they live outside the
//! daemon's kill-on-close job: closing PHPev must not close the user's windows.

use std::io;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn spawn(cmd: &mut Command, flags: u32) -> io::Result<()> {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB)
        .spawn()
        .map(|_| ())
}

/// Opens `url` (http/https only) in the default browser.
pub fn open_url(url: &str) -> io::Result<()> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "only http(s) URLs can be opened",
        ));
    }
    // url.dll hands the URL to the registered handler without a shell parsing it.
    spawn(
        Command::new(system32("rundll32.exe")).args(["url.dll,FileProtocolHandler", url]),
        CREATE_NO_WINDOW,
    )
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
