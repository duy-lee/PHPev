//! PHPev background process: supervises MariaDB and the php-cgi pools, answers
//! `phpev` over a named pipe. The HTTP router joins in M5.
//!
//! GUI subsystem: no console window, so closing a terminal never kills it.

#![windows_subsystem = "windows"]

mod appwindow;
mod dashboard;
mod endsession;
mod fastcgi;
mod folderpick;
mod foreground;
mod http_util;
mod job;
mod jobs;
mod launcher;
mod log;
mod logs;
mod metrics;
mod pool;
mod router;
mod services;
mod settings;
mod site_fix;
mod site_page;
mod supervise;
mod tls;
mod tray;
mod updates;

use std::io;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use phpev_core::config::Config;
use phpev_core::ipc::{self, Request, Response};
use phpev_core::layout::Layout;
use phpev_core::state::State;
use phpev_core::{mariadb, php, ports, procinfo, version};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::sync::Mutex;

use crate::endsession::ShutdownRequest;
use crate::log::{log_info, log_warn};
use crate::pool::Pools;
use crate::router::Router;
use crate::services::Db;

fn main() -> ExitCode {
    tray::set_dpi_aware();
    match std::env::args().nth(1).as_deref() {
        Some(launcher::ARG) => return launcher::open_dashboard(false),
        Some(launcher::RESTART_ARG) => return launcher::open_dashboard(true),
        _ => {}
    }
    let Ok(layout) = Layout::resolve() else {
        return ExitCode::FAILURE;
    };
    let _ = layout.create_dirs();
    log::init(&layout.logs_dir().join("daemon.log"));

    if let Err(e) = job::enter_kill_on_close_job() {
        // Still usable, but children could outlive a crashed daemon.
        log_warn!("could not create the kill-on-close job: {e}");
    }

    // A few worker threads: the router does small blocking file-system calls
    // (is_file, static reads) that must not serialize concurrent requests.
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(2, 4);
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(threads)
        .thread_stack_size(256 * 1024)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            log_warn!("tokio runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(layout)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log_warn!("daemon stopped with an error: {e}");
            ExitCode::FAILURE
        }
    }
}

pub(crate) struct Daemon {
    pub(crate) layout: Layout,
    started: Instant,
    db: Mutex<Option<Db>>,
    pub(crate) pools: Arc<Pools>,
    pub(crate) metrics: Arc<metrics::Metrics>,
    router: std::sync::Mutex<ipc::RouterStatus>,
    /// The running router (HTTP); the HTTPS listener reuses it.
    http_router: std::sync::Mutex<Option<Arc<Router>>>,
    shutdown: UnboundedSender<ShutdownRequest>,
    /// Dashboard login URL (with the per-start token), or why it is not running.
    dashboard: std::sync::Mutex<Result<String, String>>,
}

async fn run(layout: Layout) -> io::Result<()> {
    // The first pipe instance doubles as the single-instance guard.
    let name = ipc::pipe_name();
    let mut server = match ServerOptions::new().first_pipe_instance(true).create(&name) {
        Ok(s) => s,
        Err(e) => {
            log_info!("another PHPev daemon is already running ({e}); exiting");
            return Ok(());
        }
    };
    log_info!("daemon started (pid {}), pipe {name}", std::process::id());

    let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel::<ShutdownRequest>();
    let daemon = Arc::new(Daemon {
        layout,
        started: Instant::now(),
        db: Mutex::new(None),
        pools: Arc::new(Pools::default()),
        metrics: Arc::new(metrics::Metrics::default()),
        router: std::sync::Mutex::new(ipc::RouterStatus::default()),
        http_router: std::sync::Mutex::new(None),
        shutdown: shutdown_tx.clone(),
        dashboard: std::sync::Mutex::new(Err("not started".into())),
    });
    tray::init(daemon.clone(), tokio::runtime::Handle::current());
    endsession::spawn(shutdown_tx);

    if State::load(&daemon.layout).mariadb_running {
        if let Err(e) = daemon.db_start().await {
            log_warn!("could not restore MariaDB: {e}");
        }
    }

    daemon.start_router().await;
    daemon.clone().start_dashboard().await;
    updates::spawn(daemon.clone());

    let reaper = daemon.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        loop {
            tick.tick().await;
            reaper.pools.reap_idle().await;
        }
    });

    loop {
        tokio::select! {
            connected = server.connect() => {
                if let Err(e) = connected {
                    log_warn!("pipe connect failed: {e}");
                    continue;
                }
                let client = std::mem::replace(&mut server, ServerOptions::new().create(&name)?);
                tokio::spawn(serve(daemon.clone(), client));
            }
            Some(ack) = shutdown_rx.recv() => {
                tray::remove();
                daemon.stop_services().await;
                log_info!("daemon stopped");
                if let Some(ack) = ack {
                    let _ = ack.send(());
                }
                return Ok(());
            }
        }
    }
}

/// One request/response exchange per connection.
async fn serve(daemon: Arc<Daemon>, pipe: NamedPipeServer) {
    let (read, mut write) = tokio::io::split(pipe);
    let mut line = String::new();
    if BufReader::new(read).read_line(&mut line).await.unwrap_or(0) == 0 {
        return;
    }
    let response = match serde_json::from_str::<Request>(&line) {
        Ok(req) => daemon.handle(req).await,
        Err(e) => Response::Error {
            message: format!("bad request: {e}"),
        },
    };
    let mut out = serde_json::to_string(&response).unwrap_or_default();
    out.push('\n');
    let _ = write.write_all(out.as_bytes()).await;
    let _ = write.flush().await;
}

impl Daemon {
    async fn handle(self: &Arc<Self>, req: Request) -> Response {
        let result = match req {
            Request::Ping => Ok("pong".to_owned()),
            Request::Status => return Response::Status(Box::new(self.status().await)),
            Request::Shutdown => {
                let _ = self.shutdown.send(None);
                Ok("PHPev daemon is stopping".to_owned())
            }
            Request::DbStart => self.db_start().await.inspect(|_| self.remember_db(true)),
            Request::DbStop => self.db_stop().await.inspect(|_| self.remember_db(false)),
            Request::DbUse { major } => self.db_use(&major).await,
            Request::PhpWarm { version } => self.php_warm(version.as_deref()).await,
            Request::EnableHttps => self.start_https().await,
            Request::DashboardUrl => self
                .dashboard
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
                .map_err(io::Error::other),
            Request::OpenDashboard { browser } => appwindow::open_dashboard(self, browser).map(|opened| match opened {
                appwindow::Opened::Launched => {
                    let port = self.config().map(|c| c.ports.dashboard).unwrap_or(7600);
                    format!("Opening the dashboard: http://127.0.0.1:{port}")
                }
                appwindow::Opened::Focused => "The dashboard is already open; brought it to the front.".to_owned(),
                appwindow::Opened::Starting => "The dashboard is opening.".to_owned(),
            }),
            Request::SetTray { enabled } => {
                tray::set_visible(enabled);
                Ok(format!("tray icon {}", if enabled { "shown" } else { "hidden" }))
            }
            Request::PhpRestart { version } => {
                let stopped = self.pools.stop_version(&version).await;
                Ok(format!(
                    "php-cgi pool for PHP {version}: stopped {stopped} workers; it restarts on the next request"
                ))
            }
        };
        match result {
            Ok(message) => Response::Ok { message },
            Err(e) => Response::Error { message: e.to_string() },
        }
    }

    fn config(&self) -> io::Result<Config> {
        Config::load(&self.layout.config_file())
    }

    pub(crate) fn remember_db(&self, running: bool) {
        if let Err(e) = State::modify(&self.layout, |s| s.mariadb_running = running) {
            log_warn!("could not save state.json: {e}");
        }
    }

    pub(crate) async fn db_start(&self) -> io::Result<String> {
        let config = self.config()?;
        let major = config.mariadb.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "no MariaDB selected. Run: phpev db install 11.4",
            )
        })?;
        let version = mariadb::installed_for_major(&self.layout, &major).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("MariaDB {major} is not installed. Run: phpev db install {major}"),
            )
        })?;
        let port = config.ports.mariadb;

        let mut db = self.db.lock().await;
        if let Some(running) = db.as_ref() {
            if running.version == version && running.port == port {
                return Ok(format!("MariaDB {version} is already running on 127.0.0.1:{port}"));
            }
            if let Some(old) = db.take() {
                old.stop().await;
            }
        }
        ensure_port_free(port, "MariaDB")?;
        let started = Instant::now();
        *db = Some(Db::start(&self.layout, &version, port).await?);
        Ok(format!(
            "MariaDB {version} running on 127.0.0.1:{port} (started in {:.1}s)",
            started.elapsed().as_secs_f64()
        ))
    }

    pub(crate) async fn db_stop(&self) -> io::Result<String> {
        match self.db.lock().await.take() {
            Some(db) => {
                let v = db.version.clone();
                db.stop().await;
                Ok(format!("MariaDB {v} stopped"))
            }
            None => Ok("MariaDB is not running".to_owned()),
        }
    }

    /// The config already names the new major (written by the CLI); restart if running.
    pub(crate) async fn db_use(&self, major: &str) -> io::Result<String> {
        let version = mariadb::installed_for_major(&self.layout, major).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("MariaDB {major} is not installed. Run: phpev db install {major}"),
            )
        })?;
        let running = self.db.lock().await.as_ref().map(|db| db.version.clone());
        if running.as_deref() == Some(version.as_str()) {
            return Ok(format!("MariaDB {version} is already the running version"));
        }
        if running.is_none() {
            return Ok(format!(
                "MariaDB {version} selected (not running; start it with `phpev db start`)"
            ));
        }
        self.db_stop().await?;
        self.db_start().await
    }

    async fn php_warm(&self, wanted: Option<&str>) -> io::Result<String> {
        let installed = php::installed(&self.layout);
        let config = self.config()?;
        let wanted = wanted
            .map(str::to_owned)
            .or(config.php)
            .or_else(|| installed.first().cloned());
        let wanted = wanted
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no PHP installed. Run: phpev php install 8.4"))?;
        let found = version::best_match(&wanted, installed.iter().map(String::as_str))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("PHP {wanted} is not installed")))?;
        let n = self.pools.ensure_min(&self.layout, found).await?;
        Ok(format!("php-cgi pool for PHP {found}: {n} workers"))
    }

    pub(crate) async fn status(&self) -> ipc::Status {
        let pid = std::process::id();
        ipc::Status {
            pid,
            uptime_secs: self.started.elapsed().as_secs(),
            memory_kb: procinfo::memory_kb(pid).unwrap_or(0),
            mariadb: self.db.lock().await.as_ref().map(Db::status),
            php_pools: self.pools.status().await,
            router: self.router.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            dashboard: match &*self.dashboard.lock().unwrap_or_else(|e| e.into_inner()) {
                // The address only; the login token stays out of status output.
                Ok(url) => Ok(url.split("/login").next().unwrap_or_default().to_owned()),
                Err(e) => Err(e.clone()),
            },
        }
    }

    /// Binds the HTTP router. A busy port is reported in `phpev status`, not
    /// fatal: MariaDB and the rest keep working (decided 2026-10-23).
    async fn start_router(&self) {
        let config = self.config().unwrap_or_default();
        let port = config.ports.http;
        let mut status = ipc::RouterStatus {
            tld: config.tld.clone(),
            port,
            ..Default::default()
        };
        match ensure_port_free(port, "the HTTP router") {
            Err(e) => status.error = Some(e.to_string()),
            Ok(()) => {
                let router = Arc::new(Router::new(
                    self.layout.clone(),
                    self.pools.clone(),
                    self.metrics.clone(),
                ));
                *self.http_router.lock().unwrap_or_else(|e| e.into_inner()) = Some(router.clone());
                match router::listen(router, port, None).await {
                    Ok(addrs) => {
                        status.listening = addrs.iter().map(ToString::to_string).collect();
                        log_info!("router listening on {}", status.listening.join(", "));
                    }
                    Err(e) => status.error = Some(format!("cannot listen on port {port}: {e}")),
                }
            }
        }
        if let Some(e) = &status.error {
            log_warn!("router not started: {e}");
        }
        *self.router.lock().unwrap_or_else(|e| e.into_inner()) = status;

        // HTTPS only once the user has secured a site (the CA exists).
        if phpev_core::ca::exists(&self.layout) {
            if let Err(e) = self.start_https().await {
                log_warn!("https not started: {e}");
            }
        }
    }

    /// Binds the HTTPS listener (idempotent). A busy port is reported in status.
    pub(crate) async fn start_https(&self) -> io::Result<String> {
        if let Some(addr) = self
            .router
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .https_listening
            .first()
        {
            return Ok(format!("HTTPS already listening on {addr}"));
        }
        let router = self
            .http_router
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| io::Error::other("the HTTP router is not running"))?;
        let config = self.config()?;
        let port = config.ports.https;
        let result = async {
            ensure_port_free(port, "HTTPS")?;
            let ca = phpev_core::ca::Ca::load_or_create(&self.layout, &config.tld)?;
            let tls = tls::server_config(router.clone(), ca)?;
            router::listen(router, port, Some(tls)).await
        }
        .await;
        let mut status = self.router.lock().unwrap_or_else(|e| e.into_inner());
        status.https_port = port;
        match result {
            Ok(addrs) => {
                status.https_listening = addrs.iter().map(ToString::to_string).collect();
                status.https_error = None;
                log_info!("https listening on {}", status.https_listening.join(", "));
                Ok(format!("HTTPS listening on {}", status.https_listening.join(", ")))
            }
            Err(e) => {
                status.https_error = Some(e.to_string());
                Err(e)
            }
        }
    }

    /// Serves the dashboard on 127.0.0.1 (§14). A busy port is reported, not fatal.
    async fn start_dashboard(self: Arc<Self>) {
        let port = self.config().map(|c| c.ports.dashboard).unwrap_or(7600);
        let result = async {
            ensure_port_free(port, "the dashboard")?;
            let token = dashboard::new_token()?;
            dashboard::listen(self.clone(), port, token).await
        }
        .await;
        match &result {
            Ok(_) => log_info!("dashboard on http://127.0.0.1:{port}"),
            Err(e) => log_warn!("dashboard not started: {e}"),
        }
        *self.dashboard.lock().unwrap_or_else(|e| e.into_inner()) = result.map_err(|e| e.to_string());
    }

    /// Stops PHPev and starts it again (new ports), reopening the dashboard.
    /// A helper process waits for this one to exit; the reply that asked for
    /// the restart goes out first.
    pub(crate) fn restart(&self) -> io::Result<()> {
        let installed = self.layout.shims().join("phpev-daemon.exe");
        let exe = if installed.is_file() {
            installed
        } else {
            std::env::current_exe()?
        };
        let mut cmd = std::process::Command::new(exe);
        cmd.arg(launcher::RESTART_ARG);
        launcher::spawn(cmd)?;
        log_info!("restarting to apply new settings");
        let shutdown = self.shutdown.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(400)).await;
            let _ = shutdown.send(None);
        });
        Ok(())
    }

    /// Stops everything but keeps `state.json`, so the next start restores MariaDB.
    async fn stop_services(&self) {
        let db = self.db.lock().await.take();
        let stop_db = async {
            if let Some(db) = db {
                db.stop().await;
            }
        };
        tokio::join!(stop_db, self.pools.stop_all());
    }
}

pub(crate) fn ensure_port_free(port: u16, what: &str) -> io::Result<()> {
    let users = ports::listeners_on(port)?;
    let Some(first) = users.first() else { return Ok(()) };
    let hint = ports::known_culprit(&first.process)
        .map(|k| format!(" — {k}"))
        .unwrap_or_default();
    Err(io::Error::new(
        io::ErrorKind::AddrInUse,
        format!(
            "port {port} for {what} is used by {} (pid {}){hint}. Stop it or change [ports] in config.toml",
            first.process, first.pid
        ),
    ))
}
