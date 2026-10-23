//! Admin-only operations, started by `phpev` through UAC (DESIGN_SPEC §1).
//! Runs without a console window; the result is reported via the exit code.

#![windows_subsystem = "windows"]

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, ExitCode};

use phpev_core::elevate::{CMD_DEFENDER_EXCLUDE, CMD_HOSTS_SYNC, CMD_SYSTEM_PATH_PREPEND};
use phpev_core::env_path::{self, Scope};
use phpev_core::hosts;

const EXIT_USAGE: u8 = 2;
const EXIT_FAILED: u8 = 1;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [CMD_SYSTEM_PATH_PREPEND, dir] => system_path_prepend(dir),
        [CMD_DEFENDER_EXCLUDE, dir] => defender_exclude(dir),
        [CMD_HOSTS_SYNC, names @ ..] => hosts_sync(names),
        _ => ExitCode::from(EXIT_USAGE),
    }
}

fn system_path_prepend(dir: &str) -> ExitCode {
    let path = Path::new(dir);
    if !path.is_absolute() || !path.is_dir() {
        return ExitCode::from(EXIT_USAGE);
    }
    let result = env_path::get_raw(Scope::System, "Path").and_then(|raw| {
        let raw = raw.unwrap_or_default();
        let updated = env_path::prepend(&raw, dir);
        if updated != raw {
            env_path::set_raw(Scope::System, "Path", &updated)?;
            env_path::broadcast_change();
        }
        Ok(())
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(EXIT_FAILED),
    }
}

/// Adds `dir` to Microsoft Defender's path exclusions (stops on-access scanning
/// of the PHP binaries on every process start).
fn defender_exclude(dir: &str) -> ExitCode {
    let path = Path::new(dir);
    if !path.is_absolute() || !path.is_dir() {
        return ExitCode::from(EXIT_USAGE);
    }
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let powershell = Path::new(&system_root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    // Single-quoted PowerShell string: the only escape is doubling `'`.
    let script = format!("Add-MpPreference -ExclusionPath '{}'", dir.replace('\'', "''"));
    let status = Command::new(powershell)
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    match status {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        _ => ExitCode::from(EXIT_FAILED),
    }
}

/// Rewrites the PHPev block of the hosts file. Every name is validated here,
/// in the elevated process, so a caller cannot inject arbitrary lines.
fn hosts_sync(names: &[&str]) -> ExitCode {
    if !names.iter().all(|n| hosts::is_valid_host(n)) {
        return ExitCode::from(EXIT_USAGE);
    }
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let file = Path::new(&system_root).join(r"System32\drivers\etc\hosts");
    let existing = match std::fs::read(&file) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return ExitCode::from(EXIT_FAILED),
    };
    let names: Vec<String> = names.iter().map(|n| (*n).to_owned()).collect();
    let updated = hosts::render(&existing, &names);
    if updated == existing {
        return ExitCode::SUCCESS;
    }
    // Write in place: the hosts file often carries ACLs that a rename-over would drop.
    match std::fs::write(&file, updated) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(EXIT_FAILED),
    }
}
