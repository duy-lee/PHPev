//! Running `phpev-elevate.exe` through UAC for the few admin-only operations.

use std::io;
use std::mem;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

use crate::wide::to_wide;

pub const HELPER_EXE: &str = "phpev-elevate.exe";

/// Subcommand understood by the helper: put a directory first on the system PATH.
pub const CMD_SYSTEM_PATH_PREPEND: &str = "system-path-prepend";

/// Subcommand understood by the helper: add a Microsoft Defender path exclusion.
pub const CMD_DEFENDER_EXCLUDE: &str = "defender-exclude";

/// Subcommand understood by the helper: undo PHPev's admin-level changes in one
/// UAC prompt for `phpev uninstall`. Arguments, any combination:
/// `--hosts` (remove the PHPev hosts block), `--system-path <dir>` (remove
/// `<dir>` from the system PATH), `--defender <dir>` (remove a Defender path
/// exclusion; repeatable).
pub const CMD_UNINSTALL_CLEANUP: &str = "uninstall-cleanup";

/// Subcommand understood by the helper: rewrite the PHPev block of the hosts
/// file with the given hostnames (none = remove the block).
pub const CMD_HOSTS_SYNC: &str = "hosts-sync";

/// The helper ships next to the calling executable.
pub fn helper_path() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    Ok(exe.with_file_name(HELPER_EXE))
}

/// Starts `exe args…` with the UAC "runas" verb and waits for it.
/// Returns the helper's exit code; a declined UAC prompt is `ErrorKind::PermissionDenied`.
pub fn run_elevated(exe: &Path, args: &[&str]) -> io::Result<u32> {
    let verb = to_wide("runas");
    let file = to_wide(exe);
    let params = to_wide(args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" "));

    let mut info: SHELLEXECUTEINFOW = unsafe { mem::zeroed() };
    info.cbSize = mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = SW_HIDE;

    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let err = io::Error::last_os_error();
        if err.raw_os_error() == Some(ERROR_CANCELLED as i32) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "UAC prompt was declined",
            ));
        }
        return Err(err);
    }
    if info.hProcess.is_null() {
        return Err(io::Error::other("elevated process handle unavailable"));
    }

    let mut code = 0u32;
    unsafe {
        WaitForSingleObject(info.hProcess, INFINITE);
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
    }
    Ok(code)
}

/// Quotes one argument for the MSVC command-line parser.
fn quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_owned();
    }
    let mut out = String::from('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::quote;

    #[test]
    fn quotes_paths_with_spaces_and_trailing_backslash() {
        assert_eq!(quote(r"C:\PHPev\shims"), r"C:\PHPev\shims");
        assert_eq!(
            quote(r"C:\Users\Lê Văn\.phpev\shims\"),
            r#""C:\Users\Lê Văn\.phpev\shims\\""#
        );
        assert_eq!(quote(r#"a"b"#), r#""a\"b""#);
        assert_eq!(quote(""), r#""""#);
    }
}
