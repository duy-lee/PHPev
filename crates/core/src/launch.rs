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

/// Opens `url` in its own app-style window (`msedge --app`) with the Edge
/// profile in `profile`, or in the default browser when Edge is missing.
/// Returns whether Edge was used.
///
/// The separate profile keeps the window out of the user's Edge: none of their
/// extensions, sync or tabs load with it, and their browser does not grow with
/// it. Measured on Home: about 145 MB whether the user's Edge runs or not (the
/// page itself is ~35 MB; the rest is Edge's own processes). Software rendering
/// saves the GPU process (~45 MB) and is plenty for this page; background work
/// Edge does for a browsing profile is turned off as well. Unknown feature
/// names are ignored by Edge, so the list can only fall out of date, not break.
pub fn open_app_window(url: &str, profile: &Path) -> io::Result<bool> {
    check_http(url)?;
    let Some(edge) = edge_exe() else {
        return open_url(url).map(|()| false);
    };
    let mut cmd = Command::new(edge);
    cmd.arg(format!("--user-data-dir={}", profile.display()))
        .args([
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-extensions",
            "--disable-sync",
            "--disable-background-networking",
            "--disable-component-update",
            "--disable-default-apps",
            "--renderer-process-limit=1",
            "--disable-gpu",
            "--disable-gpu-compositing",
            "--disable-crash-reporter",
            concat!(
                "--disable-features=msEntityExtraction,msEdgeShoppingUI,msEdgeCollections,",
                "msUndersideButton,Translate,OptimizationHints,MediaRouter"
            ),
        ])
        .arg(format!("--app={url}"));
    spawn(&mut cmd, 0).map(|()| true)
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

/// Starts `program` (an .exe, never a script: cmd would interpret `&` and
/// the like in the folder name) with folder `dir` as its argument.
pub fn open_with(program: &Path, dir: &Path) -> io::Result<()> {
    if !program.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a program"));
    }
    spawn(Command::new(program).arg(dir), 0)
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
        assert!(open_app_window("javascript:alert(1)", Path::new("x")).is_err());
    }

    #[test]
    fn edge_is_found_if_installed() {
        // On the dev machine Edge exists; elsewhere this just must not panic.
        if let Some(p) = edge_exe() {
            assert!(p.ends_with("msedge.exe"), "{}", p.display());
        }
    }
}
