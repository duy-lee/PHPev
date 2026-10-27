//! `phpev-daemon.exe --open-dashboard`, what the Start menu shortcut of
//! `setup.exe` runs: sets PHPev up for this user if needed (an all-users
//! installation reaches users who never ran `phpev setup`), starts the
//! background service and opens the dashboard. Errors go to a message box,
//! since there is no console.

use std::io;
use std::os::windows::process::CommandExt;
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use phpev_core::ipc::{self, Request, Response};
use phpev_core::layout::Layout;
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

pub const ARG: &str = "--open-dashboard";
/// Started by a daemon that is about to stop to apply new settings (ports):
/// waits for it to exit, then starts PHPev again and reopens the dashboard.
pub const RESTART_ARG: &str = "--restart";

const DAEMON_EXE: &str = "phpev-daemon.exe";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
const ERROR_ACCESS_DENIED: i32 = 5;

pub fn open_dashboard(restart: bool) -> ExitCode {
    let result = if restart {
        wait_stopped().and_then(|()| run())
    } else {
        run()
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let text = wide(&format!("PHPev could not open its dashboard:\n\n{e}"));
            let caption = wide("PHPev");
            unsafe {
                MessageBoxW(
                    std::ptr::null_mut(),
                    text.as_ptr(),
                    caption.as_ptr(),
                    MB_OK | MB_ICONERROR,
                )
            };
            ExitCode::FAILURE
        }
    }
}

fn wait_stopped() -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    while ipc::is_running() {
        if Instant::now() > deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "PHPev did not stop to apply the new settings; run `phpev stop` and `phpev start`",
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn run() -> io::Result<()> {
    let installed = |layout: &Layout| layout.shims().join(DAEMON_EXE).is_file() && layout.config_file().is_file();
    let mut layout = Layout::resolve()?;
    if !installed(&layout) {
        let phpev = std::env::current_exe()?.with_file_name("phpev.exe");
        // Its exit code reflects the doctor checks; what matters is the result.
        Command::new(phpev)
            .arg("setup")
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        layout = Layout::resolve()?;
        if !installed(&layout) {
            return Err(io::Error::other(
                "setting up PHPev failed; run `phpev setup` in a terminal to see why",
            ));
        }
    }

    if !ipc::is_running() {
        spawn(Command::new(layout.shims().join(DAEMON_EXE)))?;
        let deadline = Instant::now() + Duration::from_secs(15);
        while !ipc::is_running() {
            if Instant::now() > deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "the PHPev service did not start; see logs\\daemon.log in the PHPev folder",
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    match ipc::call(&Request::OpenDashboard { browser: false })? {
        Response::Error { message } => Err(io::Error::other(message)),
        _ => Ok(()),
    }
}

/// Starts the service outside any job of the caller (an installer may run us in one).
pub(crate) fn spawn(mut cmd: Command) -> io::Result<()> {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    match cmd.creation_flags(CREATE_BREAKAWAY_FROM_JOB).spawn() {
        Err(e) if e.raw_os_error() == Some(ERROR_ACCESS_DENIED) => cmd.creation_flags(0).spawn().map(|_| ()),
        other => other.map(|_| ()),
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
