//! Dashboard: web UI + API v1 on 127.0.0.1.
//!
//! Security (§14.3): bound to 127.0.0.1 only; exact Host check (DNS
//! rebinding); a per-start random token exchanged for an HttpOnly SameSite=
//! Strict cookie; state-changing requests need a matching Origin and a JSON
//! content type; strict CSP and no framing. No shell or code execution.

use std::collections::HashMap;
use std::convert::Infallible;
use std::io;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http::header::{self, HeaderValue};
use http::{Method, Request, Response, StatusCode};
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use phpev_core::api::{self, *};
use phpev_core::config::Config;
use phpev_core::net::Http;
use phpev_core::{doctor, mariadb, ops, php, shim, sites, version};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use crate::http_util::{full, internal, query_param, text, Body, ChannelBody};
use crate::jobs::Jobs;
use crate::log::log_warn;
use crate::logs;
use crate::Daemon;
use phpev_core::launch;

const COOKIE: &str = "phpev_session";
/// Largest JSON body the API accepts.
const MAX_BODY: usize = 64 * 1024;

pub struct Dashboard {
    daemon: Arc<Daemon>,
    port: u16,
    token: String,
    jobs: Jobs,
}

/// 256-bit random token, hex.
pub fn new_token() -> io::Result<String> {
    use windows_sys::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
    let mut buf = [0u8; 32];
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status != 0 {
        return Err(io::Error::other(format!("BCryptGenRandom failed (0x{status:08X})")));
    }
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// Binds 127.0.0.1:`port` and serves the dashboard. Returns the login URL.
pub async fn listen(daemon: Arc<Daemon>, port: u16, token: String) -> io::Result<String> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?;
    let url = format!("http://127.0.0.1:{port}/login?token={token}");
    let dashboard = Arc::new(Dashboard {
        daemon,
        port,
        token,
        jobs: Jobs::default(),
    });
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let dashboard = dashboard.clone();
            tokio::spawn(async move {
                let service = service_fn(move |req| {
                    let dashboard = dashboard.clone();
                    async move { Ok::<_, Infallible>(secured(handle(&dashboard, req).await)) }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    });
    Ok(url)
}

/// Security headers on every response.
fn secured(mut res: Response<Body>) -> Response<Body> {
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"),
    );
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    h.entry(header::CACHE_CONTROL)
        .or_insert(HeaderValue::from_static("no-store"));
    res
}

fn constant_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl Dashboard {
    fn host_ok(&self, req: &Request<Incoming>) -> bool {
        let host = req
            .headers()
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        host == format!("127.0.0.1:{}", self.port) || host == format!("localhost:{}", self.port)
    }

    fn origin_ok(&self, req: &Request<Incoming>) -> bool {
        let origin = req
            .headers()
            .get(header::ORIGIN)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        origin == format!("http://127.0.0.1:{}", self.port) || origin == format!("http://localhost:{}", self.port)
    }

    fn authenticated(&self, req: &Request<Incoming>) -> bool {
        req.headers()
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(';'))
            .filter_map(|c| c.trim().split_once('='))
            .any(|(k, v)| k == COOKIE && constant_eq(v, &self.token))
    }
}

async fn handle(d: &Dashboard, req: Request<Incoming>) -> Response<Body> {
    if !d.host_ok(&req) {
        return text(StatusCode::MISDIRECTED_REQUEST, "wrong Host");
    }
    let path = req.uri().path().to_owned();

    if path == "/login" && req.method() == Method::GET {
        let query = req.uri().query().unwrap_or("");
        let token = query.split('&').find_map(|kv| kv.strip_prefix("token="));
        // Optional landing page (`to=sites`), letters only: no open redirect.
        let to = query
            .split('&')
            .find_map(|kv| kv.strip_prefix("to="))
            .filter(|p| !p.is_empty() && p.len() <= 32 && p.bytes().all(|b| b.is_ascii_lowercase()));
        let location = to.map_or_else(|| "/".to_owned(), |p| format!("/#/{p}"));
        if !token.is_some_and(|t| constant_eq(t, &d.token)) {
            return text(
                StatusCode::UNAUTHORIZED,
                "Invalid or expired link. Run `phpev dashboard` again.",
            );
        }
        // Exchange the token for a cookie and drop it from the address bar.
        return Response::builder()
            .status(StatusCode::SEE_OTHER)
            .header(header::LOCATION, location)
            .header(
                header::SET_COOKIE,
                format!("{COOKIE}={}; HttpOnly; SameSite=Strict; Path=/", d.token),
            )
            .body(full(Bytes::new()))
            .unwrap_or_else(|_| internal());
    }
    if !d.authenticated(&req) {
        return text(
            StatusCode::UNAUTHORIZED,
            "Not signed in. Open the dashboard with `phpev dashboard`.",
        );
    }
    if req.method() != Method::GET {
        if !d.origin_ok(&req) {
            return text(StatusCode::FORBIDDEN, "cross-origin request refused");
        }
        let json = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("application/json"));
        if !json {
            return text(StatusCode::UNSUPPORTED_MEDIA_TYPE, "application/json required");
        }
    }

    if let Some(api_path) = path.strip_prefix(api::PREFIX) {
        let api_path = api_path.to_owned();
        return match route(d, req, &api_path).await {
            Ok(res) => res,
            Err(e) => error_response(e),
        };
    }
    asset(&path)
}

// ---------------------------------------------------------------- assets

/// Embedded UI files (no CDN, served from memory).
fn asset(path: &str) -> Response<Body> {
    let (body, mime): (&'static [u8], &str) = match path {
        "/" | "/index.html" => (include_bytes!("../dashboard/index.html"), "text/html; charset=utf-8"),
        "/app.js" => (include_bytes!("../dashboard/app.js"), "text/javascript; charset=utf-8"),
        "/app.css" => (include_bytes!("../dashboard/app.css"), "text/css; charset=utf-8"),
        "/i18n/en.json" => (include_bytes!("../dashboard/i18n/en.json"), "application/json"),
        "/vendor/uPlot.iife.min.js" => (
            include_bytes!("../dashboard/vendor/uPlot.iife.min.js"),
            "text/javascript; charset=utf-8",
        ),
        "/vendor/uPlot.min.css" => (
            include_bytes!("../dashboard/vendor/uPlot.min.css"),
            "text/css; charset=utf-8",
        ),
        "/vendor/htm-preact-standalone.mjs" => (
            include_bytes!("../dashboard/vendor/htm-preact-standalone.mjs"),
            "text/javascript; charset=utf-8",
        ),
        "/favicon.ico" => (include_bytes!("../../../assets/web/favicon.ico"), "image/x-icon"),
        _ => return text(StatusCode::NOT_FOUND, "Not Found"),
    };
    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "no-cache")
        .body(full(Bytes::from_static(body)))
        .unwrap_or_else(|_| internal())
}

// ---------------------------------------------------------------- API

struct ApiError(StatusCode, String);

impl From<io::Error> for ApiError {
    fn from(e: io::Error) -> Self {
        let status = match e.kind() {
            io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
            io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => StatusCode::BAD_REQUEST,
            io::ErrorKind::AlreadyExists => StatusCode::CONFLICT,
            io::ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        ApiError(status, e.to_string())
    }
}

fn error_response(e: ApiError) -> Response<Body> {
    json(e.0, &ErrorBody { error: e.1 })
}

fn json<T: Serialize>(status: StatusCode, value: &T) -> Response<Body> {
    match serde_json::to_vec(value) {
        Ok(bytes) => Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "application/json")
            .body(full(bytes))
            .unwrap_or_else(|_| internal()),
        Err(_) => internal(),
    }
}

fn ok<T: Serialize>(value: &T) -> Result<Response<Body>, ApiError> {
    Ok(json(StatusCode::OK, value))
}

fn message(msg: impl Into<String>) -> Result<Response<Body>, ApiError> {
    ok(&Message { message: msg.into() })
}

async fn body<T: DeserializeOwned>(req: Request<Incoming>) -> Result<T, ApiError> {
    let bytes = req
        .into_body()
        .collect()
        .await
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))?
        .to_bytes();
    if bytes.len() > MAX_BODY {
        return Err(ApiError(StatusCode::PAYLOAD_TOO_LARGE, "request body too large".into()));
    }
    serde_json::from_slice(&bytes).map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("bad JSON: {e}")))
}

/// Runs blocking work (file system, registry, UAC/trust dialogs) off the runtime.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> io::Result<T> + Send + 'static) -> Result<T, ApiError> {
    Ok(tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??)
}

async fn route(d: &Dashboard, req: Request<Incoming>, path: &str) -> Result<Response<Body>, ApiError> {
    let segs: Vec<String> = path.trim_matches('/').split('/').map(|s| s.to_owned()).collect();
    let segs: Vec<&str> = segs.iter().map(String::as_str).collect();
    let method = req.method().clone();
    let layout = d.daemon.layout.clone();

    match (method, segs.as_slice()) {
        (Method::GET, ["overview"]) => {
            let status = d.daemon.status().await;
            let l = layout.clone();
            let checks = blocking(move || {
                let config = Config::load(&l.config_file())?;
                Ok(doctor::run(&l, &config).0)
            })
            .await?;
            ok(&Overview {
                version: env!("CARGO_PKG_VERSION").into(),
                status,
                checks,
            })
        }

        // ---- sites
        (Method::GET, ["sites"]) => {
            let l = layout.clone();
            ok(&blocking(move || site_list(&l)).await?)
        }
        (Method::POST, ["sites"]) => {
            let r: LinkRequest = body(req).await?;
            let l = layout.clone();
            let linked = blocking(move || {
                let dir = PathBuf::from(&r.path);
                ops::link_site(&l, &dir, r.name.as_deref(), r.php)
            })
            .await?;
            sync_hosts_if_needed(&layout).await;
            message(format!("Linked {}", linked.name))
        }
        (Method::DELETE, ["sites", name]) => {
            let (l, n) = (layout.clone(), name.to_string());
            match blocking(move || ops::unlink_site(&l, &n)).await? {
                Some(s) => {
                    sync_hosts_if_needed(&layout).await;
                    message(format!("Unlinked {}", s.name))
                }
                None => Err(ApiError(
                    StatusCode::NOT_FOUND,
                    format!("`{name}` is not a linked site (parked sites cannot be unlinked)"),
                )),
            }
        }
        (Method::PUT, ["sites", name, "secure"]) => {
            let r: SecureRequest = body(req).await?;
            let (l, n) = (layout.clone(), name.to_string());
            // Turning HTTPS on may show the Windows "trust this CA" dialog.
            blocking(move || ops::set_site_secure(&l, &n, r.secure, &mut ops::Quiet)).await?;
            if r.secure {
                d.daemon.start_https().await?;
            }
            sync_hosts_if_needed(&layout).await;
            message(if r.secure {
                format!("HTTPS on for {name}")
            } else {
                format!("HTTPS off for {name}")
            })
        }
        (Method::PUT, ["sites", name, "php"]) => {
            let r: PhpPinRequest = body(req).await?;
            let (l, n) = (layout.clone(), name.to_string());
            blocking(move || ops::set_site_php(&l, &n, r.version)).await?;
            message(format!("PHP for {name} updated"))
        }
        (Method::POST, ["sites", name, "open"]) => {
            let r: OpenRequest = body(req).await?;
            let (l, n) = (layout.clone(), name.to_string());
            blocking(move || {
                let site = ops::find_site(&l, &n)?;
                match r.target {
                    OpenTarget::Browser => launch::open_url(&site_url(&l, &site)?),
                    OpenTarget::Folder => launch::open_folder(&site.path),
                    OpenTarget::Terminal => launch::open_terminal(&site.path),
                }
            })
            .await?;
            message("opened")
        }

        // ---- PHP
        (Method::GET, ["php"]) => {
            let pools = d.daemon.pools.status().await;
            let l = layout.clone();
            ok(&blocking(move || {
                let config = Config::load(&l.config_file())?;
                let installed = php::installed(&l);
                let default_version = config
                    .php
                    .as_deref()
                    .and_then(|w| version::best_match(w, installed.iter().map(String::as_str)))
                    .map(str::to_owned);
                Ok(PhpInfo {
                    default: config.php.clone(),
                    installed: installed
                        .iter()
                        .map(|v| PhpVersionInfo {
                            version: v.clone(),
                            default: default_version.as_deref() == Some(v.as_str()),
                            workers: pools.iter().find(|p| &p.version == v).map_or(0, |p| p.workers.len()),
                        })
                        .collect(),
                })
            })
            .await?)
        }
        (Method::GET, ["php", "available"]) => {
            let l = layout.clone();
            ok(&blocking(move || {
                let installed = php::installed(&l);
                Ok(php::available(&Http::new()?)?
                    .into_iter()
                    .map(|r| AvailablePhp {
                        installed: installed.contains(&r.version),
                        line: r.line,
                        version: r.version,
                    })
                    .collect::<Vec<_>>())
            })
            .await?)
        }
        (Method::POST, ["php", "install"]) => {
            let r: VersionRequest = body(req).await?;
            let l = layout.clone();
            let job = d.jobs.start(format!("Install PHP {}", r.version), move |rep| {
                let v = ops::install_php(&Http::new()?, &l, &r.version, rep)?;
                Ok(format!("PHP {v} installed"))
            });
            ok(&JobStarted { job })
        }
        (Method::PUT, ["php", "default"]) => {
            let r: VersionRequest = body(req).await?;
            let l = layout.clone();
            let found = blocking(move || ops::set_default_php(&l, &r.version)).await?;
            message(format!("Default PHP is now {found}"))
        }
        (Method::GET, ["php", v, "extensions"]) => {
            let (l, v) = (layout.clone(), v.to_string());
            ok(&blocking(move || ops::extensions(&l, &v)).await?)
        }
        (Method::PUT, ["php", v, "extensions", ext]) => {
            let r: ExtensionRequest = body(req).await?;
            let (l, v2, e) = (layout.clone(), v.to_string(), ext.to_string());
            blocking(move || ops::set_extension(&l, &v2, &e, r.enabled)).await?;
            // Workers loaded the old php.ini; the pool restarts on the next request.
            d.daemon.pools.stop_version(v).await;
            message(format!(
                "{ext} {} for PHP {v}",
                if r.enabled { "enabled" } else { "disabled" }
            ))
        }
        (Method::POST, ["php", v, "restart"]) => {
            let n = d.daemon.pools.stop_version(v).await;
            message(format!(
                "Stopped {n} workers of PHP {v}; they restart on the next request"
            ))
        }

        // ---- MariaDB
        (Method::GET, ["mariadb"]) => {
            let running = d.daemon.status().await.mariadb;
            let l = layout.clone();
            ok(&blocking(move || {
                let config = Config::load(&l.config_file())?;
                Ok(MariaDbInfo {
                    installed: mariadb::installed(&l),
                    data_dir: config
                        .mariadb
                        .as_deref()
                        .map(|m| mariadb::data_dir(&l, m).display().to_string()),
                    active: config.mariadb,
                    port: config.ports.mariadb,
                    running,
                })
            })
            .await?)
        }
        (Method::GET, ["mariadb", "available"]) => {
            let l = layout.clone();
            ok(&blocking(move || {
                let installed = mariadb::installed(&l);
                let today = phpev_core::clock::today();
                Ok(mariadb::majors(&Http::new()?)?
                    .into_iter()
                    .map(|m| AvailableMariaDb {
                        installed: installed.iter().any(|v| mariadb::major_of(v) == m.release_id),
                        lts: m.is_lts(),
                        eol_passed: m.is_eol(&today),
                        eol: m.release_eol_date.clone(),
                        major: m.release_id,
                    })
                    .collect::<Vec<_>>())
            })
            .await?)
        }
        (Method::POST, ["mariadb", "install"]) => {
            let r: VersionRequest = body(req).await?;
            let l = layout.clone();
            let job = d.jobs.start(format!("Install MariaDB {}", r.version), move |rep| {
                let v = ops::install_mariadb(&Http::new()?, &l, &r.version, rep)?;
                Ok(format!("MariaDB {v} installed"))
            });
            ok(&JobStarted { job })
        }
        (Method::POST, ["mariadb", "start"]) => {
            let msg = d.daemon.db_start().await?;
            d.daemon.remember_db(true);
            message(msg)
        }
        (Method::POST, ["mariadb", "stop"]) => {
            let msg = d.daemon.db_stop().await?;
            d.daemon.remember_db(false);
            message(msg)
        }
        (Method::PUT, ["mariadb", "active"]) => {
            let r: VersionRequest = body(req).await?;
            let l = layout.clone();
            let wanted = r.version.clone();
            let (_, _previous) = blocking(move || ops::set_active_mariadb(&l, &wanted)).await?;
            message(d.daemon.db_use(&mariadb::major_of(&r.version)).await?)
        }

        // ---- performance (M9)
        // Held open by every dashboard page: when it drops, PHPev has stopped
        // and the page closes its window (tray "Exit", `phpev stop`).
        (Method::GET, ["events"]) => Ok(events_stream()),

        (Method::GET, ["metrics", "requests"]) => {
            let uri = req.uri();
            let window = query_param(uri, "window")
                .and_then(|w| w.parse().ok())
                .unwrap_or(60u32)
                .clamp(10, 600);
            let host = query_param(uri, "host").filter(|h| !h.is_empty());
            let include_static = query_param(uri, "static").as_deref() == Some("1");
            ok(&d.daemon.metrics.stats(window, host.as_deref(), include_static))
        }
        (Method::GET, ["metrics", "stream"]) => {
            let host = query_param(req.uri(), "host").filter(|h| !h.is_empty());
            let include_static = query_param(req.uri(), "static").as_deref() == Some("1");
            Ok(metrics_stream(d.daemon.clone(), host, include_static))
        }

        // ---- logs (M9)
        (Method::GET, ["logs"]) => {
            let l = layout.clone();
            ok(&blocking(move || logs::sources(&l)).await?)
        }
        (Method::GET, ["logs", "stream"]) => {
            let source = query_param(req.uri(), "source").unwrap_or_default();
            let lines = query_param(req.uri(), "lines")
                .and_then(|n| n.parse().ok())
                .unwrap_or(200usize)
                .min(2000);
            let l = layout.clone();
            let path = blocking(move || logs::resolve(&l, &source)).await?;
            Ok(log_stream(path, lines))
        }

        // ---- jobs
        (Method::GET, ["jobs", id]) => {
            let job = find_job(d, id)?;
            ok(&job.snapshot())
        }
        (Method::GET, ["jobs", id, "stream"]) => job_stream(find_job(d, id)?),

        _ => Err(ApiError(StatusCode::NOT_FOUND, format!("no API route {path}"))),
    }
}

fn find_job(d: &Dashboard, id: &str) -> Result<Arc<crate::jobs::Job>, ApiError> {
    id.parse()
        .ok()
        .and_then(|id| d.jobs.get(id))
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "no such job".into()))
}

/// Server-sent events: the job's state on every change until it finishes.
fn job_stream(job: Arc<crate::jobs::Job>) -> Result<Response<Body>, ApiError> {
    let (tx, rx) = mpsc::channel::<Bytes>(16);
    let mut changes = job.subscribe();
    tokio::spawn(async move {
        loop {
            let info = job.snapshot();
            let done = info.status != JobStatus::Running;
            let Ok(data) = serde_json::to_string(&info) else { return };
            if tx.send(Bytes::from(format!("data: {data}\n\n"))).await.is_err() || done {
                return;
            }
            // Wake on a change, or every 15 s to keep the connection alive.
            match tokio::time::timeout(Duration::from_secs(15), changes.changed()).await {
                Ok(Ok(())) | Err(_) => {}
                Ok(Err(_)) => return,
            }
        }
    });
    Ok(sse(rx))
}

/// Sends nothing but a keep-alive comment every 15 s; its end is the event.
fn events_stream() -> Response<Body> {
    let (tx, rx) = mpsc::channel(1);
    tokio::spawn(async move {
        // The first comment makes the browser report the stream as open.
        while tx.send(Bytes::from_static(b": ok\n\n")).await.is_ok() {
            tokio::time::sleep(Duration::from_secs(15)).await;
        }
    });
    sse(rx)
}

fn sse(rx: mpsc::Receiver<Bytes>) -> Response<Body> {
    Response::builder()
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-store")
        .body(ChannelBody(rx).boxed())
        .unwrap_or_else(|_| internal())
}

fn event<T: Serialize>(value: &T) -> Option<Bytes> {
    serde_json::to_string(value)
        .ok()
        .map(|json| Bytes::from(format!("data: {json}\n\n")))
}

/// Live performance: request stats and per-process CPU/RAM every second, only
/// while a client is connected (§14.4).
fn metrics_stream(daemon: Arc<Daemon>, host: Option<String>, include_static: bool) -> Response<Body> {
    let (tx, rx) = mpsc::channel::<Bytes>(4);
    tokio::spawn(async move {
        let mut cpu = CpuSampler::default();
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tick.tick().await;
            let requests = daemon.metrics.stats(60, host.as_deref(), include_static);
            let processes = cpu.sample(&daemon).await;
            let Some(ev) = event(&LiveMetrics { requests, processes }) else {
                return;
            };
            if tx.send(ev).await.is_err() {
                return; // client went away
            }
        }
    });
    sse(rx)
}

/// CPU share per process from the change in its CPU time between samples.
#[derive(Default)]
struct CpuSampler {
    previous: HashMap<u32, (u64, Instant)>,
}

impl CpuSampler {
    async fn sample(&mut self, daemon: &Daemon) -> Vec<ProcessSample> {
        let status = daemon.status().await;
        let mut procs: Vec<(String, u32)> = vec![("daemon".into(), status.pid)];
        if let Some(pid) = status.mariadb.as_ref().and_then(|m| m.pid) {
            procs.push(("mariadb".into(), pid));
        }
        for pool in &status.php_pools {
            for w in &pool.workers {
                procs.push((format!("php-cgi {}", pool.version), w.pid));
            }
        }
        let now = Instant::now();
        let mut seen = HashMap::new();
        let out = procs
            .into_iter()
            .map(|(name, pid)| {
                let ticks = phpev_core::procinfo::cpu_time(pid);
                let cpu_percent = match (ticks, self.previous.get(&pid)) {
                    (Some(t), Some((pt, pi))) => {
                        // CPU time is in 100 ns units; 100% = one full core.
                        let wall = now.duration_since(*pi).as_nanos() as f64 / 100.0;
                        if wall > 0.0 {
                            (t.saturating_sub(*pt) as f64 / wall * 100.0).max(0.0)
                        } else {
                            0.0
                        }
                    }
                    _ => 0.0,
                };
                if let Some(t) = ticks {
                    seen.insert(pid, (t, now));
                }
                ProcessSample {
                    name,
                    pid,
                    cpu_percent,
                    memory_kb: phpev_core::procinfo::memory_kb(pid),
                }
            })
            .collect();
        self.previous = seen;
        out
    }
}

/// Tails a log: the last `lines`, then new lines every 500 ms.
fn log_stream(path: PathBuf, lines: usize) -> Response<Body> {
    let (tx, rx) = mpsc::channel::<Bytes>(8);
    tokio::spawn(async move {
        let p = path.clone();
        let Ok(Ok((first, mut offset))) = tokio::task::spawn_blocking(move || logs::tail(&p, lines)).await else {
            return;
        };
        let Some(ev) = event(&LogChunk {
            lines: first,
            reset: false,
        }) else {
            return;
        };
        if tx.send(ev).await.is_err() {
            return;
        }
        let mut idle = 0u32;
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let p = path.clone();
            let Ok(Ok((new, next, reset))) = tokio::task::spawn_blocking(move || logs::read_new(&p, offset)).await
            else {
                return;
            };
            offset = next;
            if new.is_empty() && !reset {
                // A comment every ~15 s detects closed connections.
                idle += 1;
                if idle.is_multiple_of(30) && tx.send(Bytes::from_static(b": keep-alive\n\n")).await.is_err() {
                    return;
                }
                continue;
            }
            idle = 0;
            let Some(ev) = event(&LogChunk { lines: new, reset }) else {
                return;
            };
            if tx.send(ev).await.is_err() {
                return;
            }
        }
    });
    sse(rx)
}

pub(crate) fn site_url(layout: &phpev_core::layout::Layout, site: &sites::Site) -> io::Result<String> {
    let config = Config::load(&layout.config_file())?;
    let (scheme, port) = if site.secure {
        (
            "https",
            if config.ports.https == 443 {
                String::new()
            } else {
                format!(":{}", config.ports.https)
            },
        )
    } else {
        (
            "http",
            if config.ports.http == 80 {
                String::new()
            } else {
                format!(":{}", config.ports.http)
            },
        )
    };
    Ok(format!("{scheme}://{}{port}", site.host(&config.tld)))
}

fn site_list(layout: &phpev_core::layout::Layout) -> io::Result<Vec<SiteInfo>> {
    let config = Config::load(&layout.config_file())?;
    sites::all(&config)
        .iter()
        .map(|s| {
            Ok(SiteInfo {
                url: site_url(layout, s)?,
                name: s.name.clone(),
                kind: if s.kind == sites::Kind::Linked {
                    "linked"
                } else {
                    "parked"
                }
                .into(),
                path: s.path.display().to_string(),
                php_pinned: s.php.clone(),
                php: shim::resolve(layout.root(), &s.path)
                    .map(|r| r.version)
                    .map_err(|e| e.to_string()),
                secure: s.secure,
            })
        })
        .collect()
}

/// With a TLD other than localhost, keep the hosts file in sync (UAC). A
/// declined prompt only logs: the site change itself already happened.
async fn sync_hosts_if_needed(layout: &phpev_core::layout::Layout) {
    let l = layout.clone();
    let result = tokio::task::spawn_blocking(move || {
        let config = Config::load(&l.config_file())?;
        if ops::needs_hosts(&config) {
            ops::write_hosts(&config, true)?;
        }
        Ok::<_, io::Error>(())
    })
    .await;
    if let Ok(Err(e)) = result {
        log_warn!("hosts file not updated: {e}");
    }
}
