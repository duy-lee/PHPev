//! `phpev start|stop|status|autostart`, `phpev db start|stop|use`, `phpev php warm`.

use std::io;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use clap::ValueEnum;
use phpev_core::config;
use phpev_core::ipc::{self, Request, Response};
use phpev_core::layout::Layout;
use phpev_core::mariadb;
use phpev_core::state::State;
use phpev_core::{autostart, launch};

use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};

use crate::progress::mib;

const DAEMON_EXE: &str = "phpev-daemon.exe";

const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
const ERROR_ACCESS_DENIED: i32 = 5;

/// Starts the daemon unless it is running. Returns whether it was started now.
pub(crate) fn ensure_running() -> io::Result<bool> {
    if ipc::is_running() {
        return Ok(false);
    }
    let exe = std::env::current_exe()?.with_file_name(DAEMON_EXE);
    spawn_detached(&exe)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if ipc::is_running() {
            return Ok(true);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "the PHPev daemon did not come up; see logs\\daemon.log",
    ))
}

/// Starts the daemon outside this terminal's process tree and job, so closing
/// the terminal (VS Code, Windows Terminal) does not take it down.
fn spawn_detached(exe: &PathBuf) -> io::Result<()> {
    // Windows children inherit every inheritable handle, not just the three
    // passed as stdio. If our stdout is a pipe (`phpev start | findstr`, IDE
    // task runners), a daemon holding its write end would keep the reader
    // waiting for EOF until the daemon exits.
    unsafe {
        for std in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            let h = GetStdHandle(std);
            if !h.is_null() && h != INVALID_HANDLE_VALUE {
                SetHandleInformation(h, HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
    let spawn = |flags: u32| {
        Command::new(exe)
            .creation_flags(flags)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    };
    let base = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    match spawn(base | CREATE_BREAKAWAY_FROM_JOB) {
        // The terminal's job does not allow breakaway; start inside it.
        Err(e) if e.raw_os_error() == Some(ERROR_ACCESS_DENIED) => spawn(base).map(|_| ()),
        other => other.map(|_| ()),
    }
}

/// Sends an action; daemon-side errors become `Err`.
pub(crate) fn request(req: &Request) -> io::Result<String> {
    match ipc::call(req)? {
        Response::Ok { message } => Ok(message),
        Response::Error { message } => Err(io::Error::other(message)),
        Response::Status(_) => Err(io::Error::other("unexpected status reply")),
    }
}

pub fn start() -> io::Result<ExitCode> {
    if ensure_running()? {
        println!("PHPev daemon started.");
    } else {
        println!("PHPev daemon is already running.");
    }
    status()
}

pub fn stop() -> io::Result<ExitCode> {
    if !ipc::is_running() {
        println!("PHPev daemon is not running.");
        return Ok(ExitCode::SUCCESS);
    }
    request(&Request::Shutdown)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    while ipc::is_running() {
        if Instant::now() > deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the daemon is still running after 30 s",
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    println!("PHPev daemon stopped.");
    Ok(ExitCode::SUCCESS)
}

pub fn status() -> io::Result<ExitCode> {
    let autostart = autostart::is_enabled()?;
    let status = match ipc::call(&Request::Status) {
        Ok(Response::Status(s)) => *s,
        Ok(_) => return Err(io::Error::other("unexpected reply")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            println!("PHPev daemon: not running  (start it with `phpev start`)");
            println!("Autostart:    {}", if autostart { "on" } else { "off" });
            return Ok(ExitCode::SUCCESS);
        }
        Err(e) => return Err(e),
    };

    println!(
        "PHPev daemon: running  pid {}  up {}  RAM {}",
        status.pid,
        uptime(status.uptime_secs),
        mib(status.memory_kb * 1024)
    );
    let r = &status.router;
    let print_https = || match (&r.https_error, r.https_listening.is_empty()) {
        (Some(e), _) => println!("HTTPS:        NOT RUNNING — {e}"),
        (None, false) => println!("HTTPS:        secured sites  ({})", r.https_listening.join(", ")),
        (None, true) => println!("HTTPS:        off (enable with `phpev secure <site>`)"),
    };
    match (&r.error, r.listening.is_empty()) {
        (Some(e), _) => println!("Router:       NOT RUNNING — {e}"),
        (None, false) => {
            let port = if r.port == 80 {
                String::new()
            } else {
                format!(":{}", r.port)
            };
            println!(
                "Router:       http://<site>.{}{port}  ({})",
                r.tld,
                r.listening.join(", ")
            );
        }
        (None, true) => println!("Router:       not started"),
    }
    print_https();
    match &status.dashboard {
        Ok(addr) => println!("Dashboard:    {addr}  (open with `phpev dashboard`)"),
        Err(e) => println!("Dashboard:    NOT RUNNING — {e}"),
    }
    match &status.mariadb {
        Some(db) => println!(
            "MariaDB:      {} {}  127.0.0.1:{}  pid {}  RAM {}",
            db.version,
            db.state,
            db.port,
            db.pid.map_or("-".into(), |p| p.to_string()),
            db.memory_kb.map_or("-".into(), |k| mib(k * 1024))
        ),
        None => println!("MariaDB:      stopped"),
    }
    if status.php_pools.is_empty() {
        println!("PHP pools:    none (started on first request, or `phpev php warm`)");
    }
    for pool in &status.php_pools {
        let ram: u64 = pool.workers.iter().filter_map(|w| w.memory_kb).sum();
        println!(
            "PHP {}:    {} php-cgi workers  RAM {}",
            pool.version,
            pool.workers.len(),
            mib(ram * 1024)
        );
        for w in &pool.workers {
            println!("                pid {}  127.0.0.1:{}", w.pid, w.port);
        }
    }
    println!("Autostart:    {}", if autostart { "on" } else { "off" });
    Ok(ExitCode::SUCCESS)
}

fn uptime(secs: u64) -> String {
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m{}s", s / 60, s % 60),
        s => format!("{}h{}m", s / 3600, s / 60 % 60),
    }
}

pub fn db_start() -> io::Result<ExitCode> {
    ensure_running()?;
    println!("{}", request(&Request::DbStart)?);
    Ok(ExitCode::SUCCESS)
}

pub fn db_stop() -> io::Result<ExitCode> {
    if ipc::is_running() {
        println!("{}", request(&Request::DbStop)?);
    } else {
        // The daemon is the usual writer of state.json, but it is not running.
        let layout = Layout::resolve()?;
        let mut state = State::load(&layout);
        state.mariadb_running = false;
        state.save(&layout)?;
        println!("MariaDB is not running.");
    }
    Ok(ExitCode::SUCCESS)
}

pub fn db_use(wanted: &str) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    let major = mariadb::major_of(wanted);
    let version = mariadb::installed_for_major(&layout, &major).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("MariaDB {major} is not installed. Run: phpev db install {major}"),
        )
    })?;
    let previous = config::update(&layout.config_file(), |c| c.mariadb.replace(major.clone()))?;

    if ipc::is_running() {
        println!("{}", request(&Request::DbUse { major: major.clone() })?);
    } else {
        println!("MariaDB {version} selected.");
    }
    if let Some(prev) = previous.filter(|p| *p != major) {
        println!(
            "note: each major version keeps its own databases. {prev} stays in {}; {major} uses {}. Nothing was copied.",
            mariadb::data_dir(&layout, &prev).display(),
            mariadb::data_dir(&layout, &major).display()
        );
    }
    Ok(ExitCode::SUCCESS)
}

pub fn php_warm(version: Option<String>) -> io::Result<ExitCode> {
    ensure_running()?;
    println!("{}", request(&Request::PhpWarm { version })?);
    Ok(ExitCode::SUCCESS)
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Switch {
    On,
    Off,
}

pub fn autostart(mode: Switch) -> io::Result<ExitCode> {
    let on = matches!(mode, Switch::On);
    autostart::set(&Layout::resolve()?, on)?;
    println!(
        "{}",
        if on {
            "PHPev will start when you log in to Windows."
        } else {
            "PHPev will no longer start at log in."
        }
    );
    Ok(ExitCode::SUCCESS)
}

/// `phpev tray on|off`: remembered in config.toml and applied to the running daemon.
pub fn tray(mode: Switch) -> io::Result<ExitCode> {
    let on = matches!(mode, Switch::On);
    let layout = Layout::resolve()?;
    config::update(&layout.config_file(), |c| c.tray = on)?;
    if ipc::is_running() {
        request(&Request::SetTray { enabled: on })?;
    }
    println!("Tray icon {}.", if on { "on" } else { "off" });
    Ok(ExitCode::SUCCESS)
}

/// `phpev dashboard`: starts PHPev if needed and opens the dashboard with this
/// run's login token (the browser then holds it as an HttpOnly cookie), in its
/// own app window (Edge) unless `browser` asks for a normal browser tab.
pub fn dashboard(browser: bool) -> io::Result<ExitCode> {
    ensure_running()?;
    let url = request(&Request::DashboardUrl)?;
    if browser {
        launch::open_url(&url)?;
    } else {
        launch::open_app_window(&url)?;
    }
    println!("Opening the dashboard: {}", url.split("/login").next().unwrap_or(&url));
    Ok(ExitCode::SUCCESS)
}
