//! Admin-only operations, started by `phpev` through UAC.
//! Runs without a console window; the result is reported via the exit code.

#![windows_subsystem = "windows"]

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, ExitCode};

use phpev_core::elevate::{CMD_DEFENDER_EXCLUDE, CMD_HOSTS_SYNC, CMD_SYSTEM_PATH_PREPEND, CMD_UNINSTALL_CLEANUP};
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
        [CMD_UNINSTALL_CLEANUP, rest @ ..] => uninstall_cleanup(rest),
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

/// Undoes PHPev's admin-level changes (see `CMD_UNINSTALL_CLEANUP`). Every step
/// is attempted; the exit code reports whether all of them succeeded.
fn uninstall_cleanup(args: &[&str]) -> ExitCode {
    let mut hosts_block = false;
    let mut system_path = Vec::new();
    let mut defender = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match *arg {
            "--hosts" => hosts_block = true,
            "--system-path" | "--defender" => {
                let Some(dir) = it.next().filter(|d| Path::new(d).is_absolute()) else {
                    return ExitCode::from(EXIT_USAGE);
                };
                if *arg == "--defender" {
                    defender.push(*dir)
                } else {
                    system_path.push(*dir)
                }
            }
            _ => return ExitCode::from(EXIT_USAGE),
        }
    }
    let mut ok = true;
    if hosts_block {
        ok &= hosts_sync(&[]) == ExitCode::SUCCESS;
    }
    if !system_path.is_empty() {
        let result = env_path::get_raw(Scope::System, "Path").and_then(|raw| {
            let raw = raw.unwrap_or_default();
            let updated = system_path
                .iter()
                .fold(raw.clone(), |acc, dir| env_path::remove(&acc, dir));
            if updated != raw {
                env_path::set_raw(Scope::System, "Path", &updated)?;
                env_path::broadcast_change();
            }
            Ok(())
        });
        ok &= result.is_ok();
    }
    if !defender.is_empty() {
        // The folders may already be gone, so they are not required to exist.
        // Single-quoted PowerShell strings: the only escape is doubling `'`.
        let list = defender
            .iter()
            .map(|d| format!("'{}'", d.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        ok &= powershell(&format!("Remove-MpPreference -ExclusionPath {list}"));
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(EXIT_FAILED)
    }
}

fn powershell(script: &str) -> bool {
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let powershell = Path::new(&system_root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    Command::new(powershell)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .is_ok_and(|s| s.success())
}

/// Adds `dir` to Microsoft Defender's path exclusions (stops on-access scanning
/// of the PHP binaries on every process start).
fn defender_exclude(dir: &str) -> ExitCode {
    let path = Path::new(dir);
    if !path.is_absolute() || !path.is_dir() {
        return ExitCode::from(EXIT_USAGE);
    }
    // Single-quoted PowerShell string: the only escape is doubling `'`.
    if powershell(&format!(
        "Add-MpPreference -ExclusionPath '{}'",
        dir.replace('\'', "''")
    )) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(EXIT_FAILED)
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
