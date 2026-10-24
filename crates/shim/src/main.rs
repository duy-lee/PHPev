//! `php.exe` / `composer.exe` shim. One binary, copied under
//! both names into `<home>\shims`; the file name decides what it runs.
//!
//! Hot path: no config parsing, no registry, no allocation-heavy work — see
//! `phpev_core::shim::resolve`.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use phpev_core::{php, shim};
use windows_sys::Win32::Foundation::BOOL;
use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

/// Set to print how the version was chosen (to stderr) before running.
const DEBUG_ENV: &str = "PHPEV_SHIM_DEBUG";

fn main() -> ExitCode {
    match run() {
        Ok(code) => {
            // Windows exit codes are 32-bit (e.g. 0xC000013A after Ctrl+C); ExitCode only carries a u8.
            std::process::exit(code)
        }
        Err(msg) => {
            eprintln!("phpev: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<i32, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate the shim: {e}"))?;
    let tool = exe
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    // <home>\shims\php.exe → <home>
    let home = exe
        .parent()
        .and_then(Path::parent)
        .ok_or("the shim must live in <PHPev home>\\shims")?;
    let cwd = std::env::current_dir().map_err(|e| format!("cannot read the current directory: {e}"))?;

    let resolved = shim::resolve(home, &cwd).map_err(|e| e.to_string())?;
    if std::env::var_os(DEBUG_ENV).is_some() {
        eprintln!(
            "phpev: {tool} → PHP {} (wanted {} from {})",
            resolved.version, resolved.wanted, resolved.source
        );
    }

    let mut cmd = Command::new(resolved.dir.join("php.exe"));
    // PHP finds php.ini through PHPRC; child processes (artisan serve, queue
    // workers using PHP_BINARY) inherit it. Non-ASCII homes need the 8.3 form (spike S3).
    let ini_dir = php::php_path(&resolved.dir).map_err(|e| e.to_string())?;
    cmd.env("PHPRC", ini_dir);

    match tool.as_str() {
        "composer" => {
            let phar: PathBuf = home.join(r"bin\composer\composer.phar");
            if !phar.is_file() {
                return Err("Composer is not installed. Run: phpev composer install".into());
            }
            cmd.arg(phar);
        }
        "php" => {}
        other => return Err(format!("unknown shim name `{other}` (expected php or composer)")),
    }
    cmd.args(std::env::args_os().skip(1));

    // Ctrl+C reaches the whole console group: the child handles it, the shim
    // stays alive to forward the exit code. A handler (unlike `None`, which
    // child processes inherit) leaves the child's own Ctrl+C handling intact.
    unsafe { SetConsoleCtrlHandler(Some(ignore_ctrl), 1) };
    let status = cmd
        .status()
        .map_err(|e| format!("cannot start {}: {e}", resolved.dir.join("php.exe").display()))?;
    Ok(status.code().unwrap_or(1))
}

unsafe extern "system" fn ignore_ctrl(_ctrl_type: u32) -> BOOL {
    1
}
