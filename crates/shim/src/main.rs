//! `php.exe` / `composer.exe` / `mysql.exe` / `mysqldump.exe` shim (and
//! `mariadb.exe` / `mariadb-dump.exe`). One binary, copied under each name
//! into `<home>\shims`; the file name decides what it runs.
//!
//! Hot path: no config parsing, no registry, no allocation-heavy work — see
//! `phpev_core::shim::resolve`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use phpev_core::{composer, php, shim};
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

    let mut cmd = match tool.as_str() {
        "php" | "composer" => php_command(&tool, home)?,
        "mysql" | "mariadb" => mariadb_command(&tool, home, "mariadb.exe")?,
        "mysqldump" | "mariadb-dump" => mariadb_command(&tool, home, "mariadb-dump.exe")?,
        other => {
            return Err(format!(
                "unknown shim name `{other}` (expected php, composer, mysql, mysqldump, mariadb or mariadb-dump)"
            ))
        }
    };

    // Ctrl+C reaches the whole console group: the child handles it, the shim
    // stays alive to forward the exit code. A handler (unlike `None`, which
    // child processes inherit) leaves the child's own Ctrl+C handling intact.
    unsafe { SetConsoleCtrlHandler(Some(ignore_ctrl), 1) };
    let status = cmd
        .status()
        .map_err(|e| format!("cannot start {}: {e}", Path::new(cmd.get_program()).display()))?;
    Ok(status.code().unwrap_or(1))
}

/// `php` / `composer`: the PHP version of the current folder.
fn php_command(tool: &str, home: &Path) -> Result<Command, String> {
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

    if tool == "composer" {
        let phar: PathBuf = composer::phar_for(&home.join(r"bin\composer"), &resolved.version);
        if !phar.is_file() {
            return Err(if composer::needs_lts(&resolved.version) {
                format!(
                    "PHP {} needs Composer 2.2 LTS (newer Composer needs PHP 7.2.5+), which is not installed. Run: phpev composer install",
                    resolved.version
                )
            } else {
                "Composer is not installed. Run: phpev composer install".into()
            });
        }
        cmd.arg(phar);
    }
    cmd.args(std::env::args_os().skip(1));
    Ok(cmd)
}

/// `mysql` / `mysqldump` and their MariaDB names: the client of the active
/// MariaDB, connected to PHPev's server as root unless the arguments say otherwise.
fn mariadb_command(tool: &str, home: &Path, exe: &str) -> Result<Command, String> {
    let db = shim::resolve_mariadb(home)?;
    if std::env::var_os(DEBUG_ENV).is_some() {
        eprintln!("phpev: {tool} → MariaDB {} on port {}", db.version, db.port);
    }
    let mut cmd = Command::new(db.bin.join(exe));
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    // Option-file arguments are only accepted first; later options override
    // earlier ones, so the user's -h/-P/-u win over these defaults.
    let leading = args
        .iter()
        .take_while(|a| {
            let a = a.to_string_lossy();
            OPTION_FILE_ARGS.iter().any(|o| a.starts_with(o))
        })
        .count();
    cmd.args(&args[..leading]);
    cmd.args([
        "--host=127.0.0.1".to_owned(),
        format!("--port={}", db.port),
        "--user=root".to_owned(),
    ]);
    // PHPev's server has no TLS certificate and root no password: newer
    // clients warn on every call about the server certificate they cannot
    // check. Only skipped for that local server, not for a host given here.
    if !args.iter().any(|a| is_host_arg(&a.to_string_lossy())) {
        cmd.arg("--skip-ssl-verify-server-cert");
    }
    cmd.args(&args[leading..]);
    Ok(cmd)
}

/// `-h host`, `-hhost`, `--host=host` or `--host host`.
fn is_host_arg(arg: &str) -> bool {
    (arg.starts_with("-h") && !arg.starts_with("--")) || arg == "--host" || arg.starts_with("--host=")
}

/// MariaDB client options that must come before any other.
const OPTION_FILE_ARGS: &[&str] = &[
    "--no-defaults",
    "--defaults-file",
    "--defaults-extra-file",
    "--defaults-group-suffix",
    "--print-defaults",
];

unsafe extern "system" fn ignore_ctrl(_ctrl_type: u32) -> BOOL {
    1
}
