//! Admin-only operations, started by `phpev` through UAC (DESIGN_SPEC §1).
//! Runs without a console window; the result is reported via the exit code.

#![windows_subsystem = "windows"]

use std::path::Path;
use std::process::ExitCode;

use phpev_core::elevate::CMD_SYSTEM_PATH_PREPEND;
use phpev_core::env_path::{self, Scope};

const EXIT_USAGE: u8 = 2;
const EXIT_FAILED: u8 = 1;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [CMD_SYSTEM_PATH_PREPEND, dir] => system_path_prepend(dir),
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
