//! HTTP front end: Host → site → static file or FastCGI to a php-cgi worker
//! (DESIGN_SPEC §4).

use std::collections::HashMap;
use std::convert::Infallible;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use http::header::{self, HeaderName, HeaderValue};
use http::{Request, Response, StatusCode};
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use phpev_core::config::Config;
use phpev_core::layout::Layout;
use phpev_core::shim;
use phpev_core::sites::{self, Site};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use crate::fastcgi::{self, Event, Params};
use crate::http_util::{empty, escape, full, internal, text, Body, ChannelBody};
use crate::log::log_warn;
use crate::pool::{Lease, Pools};

/// Files up to this size are read in one go; larger ones are streamed.
const INLINE_FILE_MAX: u64 = 1 << 20;
const STREAM_CHUNK: usize = 64 * 1024;
/// PHP output up to this size is sent with Content-Length in one piece…
const INLINE_RESPONSE_MAX: usize = 1 << 20;
/// …unless PHP pauses longer than this between records (SSE, StreamedResponse,
/// flush() loops): then the rest is streamed as it arrives.
const STREAM_AFTER_PAUSE: Duration = Duration::from_millis(20);

pub struct Router {
    layout: Layout,
    pools: Arc<Pools>,
    cache: Mutex<SiteCache>,
    /// Site folder → (installed PHP version, when resolved). Resolving reads
    /// `.php-version` files and `bin\php`; doing it per request would put
    /// several blocking file-system calls on the hot path.
    versions: Mutex<HashMap<PathBuf, (String, Instant)>>,
}

/// How long a resolved PHP version is reused before `.php-version` is re-read.
const VERSION_TTL: Duration = Duration::from_secs(2);

/// Config and site table. Reloaded as soon as config.toml changes (one `stat`
/// per request, so `phpev link` takes effect on the very next request), and
/// re-scanned at most once per second when a host is not found (a new folder
/// in a parked directory).
struct SiteCache {
    config: Arc<Config>,
    sites: Arc<Vec<Site>>,
    mtime: Option<SystemTime>,
    scanned: Instant,
}

impl Router {
    pub fn new(layout: Layout, pools: Arc<Pools>) -> Self {
        let config = Config::load(&layout.config_file()).unwrap_or_default();
        let sites = sites::all(&config);
        let mtime = std::fs::metadata(layout.config_file()).and_then(|m| m.modified()).ok();
        let cache = SiteCache {
            config: Arc::new(config),
            sites: Arc::new(sites),
            mtime,
            scanned: Instant::now(),
        };
        Self {
            layout,
            pools,
            cache: Mutex::new(cache),
            versions: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn snapshot(&self) -> (Arc<Config>, Arc<Vec<Site>>) {
        // stat outside the lock; reload only when the file actually changed.
        let mtime = std::fs::metadata(self.layout.config_file())
            .and_then(|m| m.modified())
            .ok();
        {
            let c = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if mtime == c.mtime {
                return (c.config.clone(), c.sites.clone());
            }
        }
        let Ok(config) = Config::load(&self.layout.config_file()) else {
            let c = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            return (c.config.clone(), c.sites.clone()); // mid-write or invalid: keep the last good one
        };
        let sites = Arc::new(sites::all(&config));
        let config = Arc::new(config);
        // A pinned PHP version may have changed with the config.
        self.versions.lock().unwrap_or_else(|e| e.into_inner()).clear();
        let mut c = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        c.config = config;
        c.sites = sites;
        c.mtime = mtime;
        c.scanned = Instant::now();
        (c.config.clone(), c.sites.clone())
    }

    /// PHP version for a site, from the 2-second cache or `shim::resolve`.
    fn php_version(&self, site: &Path) -> Result<String, shim::Error> {
        if let Some((v, at)) = self.versions.lock().unwrap_or_else(|e| e.into_inner()).get(site) {
            if at.elapsed() < VERSION_TTL {
                return Ok(v.clone());
            }
        }
        let version = shim::resolve(self.layout.root(), site)?.version;
        self.versions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(site.to_owned(), (version.clone(), Instant::now()));
        Ok(version)
    }

    /// Re-reads parked directories after a miss, at most once per second.
    fn rescan(&self) -> Arc<Vec<Site>> {
        let mut c = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if c.scanned.elapsed().as_secs() >= 1 {
            c.scanned = Instant::now();
            c.sites = Arc::new(sites::all(&c.config));
        }
        c.sites.clone()
    }
}

/// Binds 127.0.0.1 and ::1 (browsers try `*.localhost` on both); never
/// 0.0.0.0, so no firewall prompt (§10). With `tls`, connections are HTTPS.
/// Returns the bound addresses.
pub async fn listen(
    router: Arc<Router>,
    port: u16,
    tls: Option<Arc<rustls::ServerConfig>>,
) -> io::Result<Vec<SocketAddr>> {
    let acceptor = tls.map(tokio_rustls::TlsAcceptor::from);
    let https = acceptor.is_some();
    let mut bound = Vec::new();
    let v4 = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?;
    bound.push(v4.local_addr()?);
    let mut listeners = vec![v4];
    match TcpListener::bind((Ipv6Addr::LOCALHOST, port)).await {
        Ok(v6) => {
            bound.push(v6.local_addr()?);
            listeners.push(v6);
        }
        Err(e) => log_warn!("router: [::1]:{port} unavailable ({e}); serving IPv4 only"),
    }
    for listener in listeners {
        let router = router.clone();
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            loop {
                let Ok((stream, remote)) = listener.accept().await else {
                    continue;
                };
                let _ = stream.set_nodelay(true);
                let router = router.clone();
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    let service = service_fn(move |req| {
                        let router = router.clone();
                        async move { Ok::<_, Infallible>(handle(&router, req, remote, https, port).await) }
                    });
                    let mut http = hyper::server::conn::http1::Builder::new();
                    match acceptor {
                        // Handshake failures (unknown host, untrusted client) just drop the connection.
                        Some(acceptor) => {
                            if let Ok(tls) = acceptor.accept(stream).await {
                                let _ = http.keep_alive(true).serve_connection(TokioIo::new(tls), service).await;
                            }
                        }
                        None => {
                            let _ = http
                                .keep_alive(true)
                                .serve_connection(TokioIo::new(stream), service)
                                .await;
                        }
                    }
                });
            }
        });
    }
    Ok(bound)
}

async fn handle(router: &Router, req: Request<Incoming>, remote: SocketAddr, https: bool, port: u16) -> Response<Body> {
    let host = request_host(&req);
    let (config, mut all) = router.snapshot();
    if host == "localhost" || host == "127.0.0.1" || host == "[::1]" {
        return dashboard(StatusCode::OK, &all, &config, None);
    }
    let mut site = sites::find(&all, &host, &config.tld).cloned();
    if site.is_none() {
        all = router.rescan();
        site = sites::find(&all, &host, &config.tld).cloned();
    }
    let Some(site) = site else {
        return dashboard(StatusCode::NOT_FOUND, &all, &config, Some(&host));
    };

    // Secured sites are HTTPS-only. 307, not 301: browsers cache 301 forever,
    // which would break the site after `phpev unsecure`.
    if site.secure && !https {
        let https_port = if config.ports.https == 443 {
            String::new()
        } else {
            format!(":{}", config.ports.https)
        };
        let target = format!(
            "https://{host}{https_port}{}",
            req.uri().path_and_query().map_or("/", |pq| pq.as_str())
        );
        return Response::builder()
            .status(StatusCode::TEMPORARY_REDIRECT)
            .header(header::LOCATION, target)
            .body(empty())
            .unwrap_or_else(|_| internal());
    }

    let docroot = site.docroot();
    let Some(rel) = sanitize(req.uri().path()) else {
        return text(StatusCode::NOT_FOUND, "Not Found");
    };
    match route(&docroot, &rel) {
        Target::Static(file) => serve_static(&req, &file).await,
        Target::Php {
            script,
            script_name,
            path_info,
        } => {
            let version = match router.php_version(&site.path) {
                Ok(v) => v,
                Err(e) => return text(StatusCode::INTERNAL_SERVER_ERROR, &format!("PHPev: {e}")),
            };
            let ctx = PhpRequest {
                docroot: &docroot,
                script: &script,
                script_name,
                path_info,
                host: &host,
                remote,
                https,
                server_port: port,
            };
            match router.pools.acquire(&router.layout, &version).await {
                Ok(lease) => run_php(&router.pools, lease, req, ctx).await,
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                    text(StatusCode::SERVICE_UNAVAILABLE, &format!("PHPev: {e}"))
                }
                Err(e) => text(
                    StatusCode::BAD_GATEWAY,
                    &format!("PHPev: cannot start PHP {version}: {e}"),
                ),
            }
        }
        Target::NotFound => text(StatusCode::NOT_FOUND, "Not Found"),
    }
}

/// Host header (or URI authority) without port, lowercase.
fn request_host(req: &Request<Incoming>) -> String {
    let raw = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .or_else(|| req.uri().authority().map(|a| a.as_str()))
        .unwrap_or("");
    let host = if raw.starts_with('[') {
        raw.split_once(']').map_or(raw, |(h, _)| h).to_owned() + "]"
    } else {
        raw.split(':').next().unwrap_or("").to_owned()
    };
    host.to_ascii_lowercase()
}

// ---------------------------------------------------------------- routing

#[derive(Debug, PartialEq, Eq)]
enum Target {
    Static(PathBuf),
    Php {
        script: PathBuf,
        script_name: String,
        path_info: String,
    },
    NotFound,
}

/// Percent-decodes the URL path into safe relative segments. `None` for
/// anything that could escape the docroot or reach hidden files: `..`,
/// dot-segments (except `.well-known`), `:` (drive letters, NTFS streams),
/// backslashes, NUL, `~` (8.3 aliases such as `ENV~1`), and trailing dots or
/// spaces (which Windows silently strips: `index.php.` opens `index.php`).
///
/// Rejecting `~` closes the 8.3 alias route to hidden files, so no canonical
/// path check is needed per request. Symlinks/junctions inside the docroot are
/// followed on purpose: Laravel's `public/storage` points outside `public/`.
fn sanitize(path: &str) -> Option<Vec<String>> {
    let decoded = percent_decode(path)?;
    let mut out = Vec::new();
    for seg in decoded.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        let hidden = seg.starts_with('.') && !(out.is_empty() && seg == ".well-known");
        if seg == ".." || hidden || seg.contains([':', '\\', '\0', '~']) || seg.ends_with(['.', ' ']) {
            return None;
        }
        out.push(seg.to_owned());
    }
    Some(out)
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Front-controller routing (Laravel and plain PHP):
/// existing file → static, or run it if `.php`; `/x.php/extra` → run x.php with
/// PATH_INFO; directory → its index.php / index.html; otherwise the
/// docroot's index.php.
fn route(docroot: &Path, rel: &[String]) -> Target {
    let mut path = docroot.to_path_buf();
    for (i, seg) in rel.iter().enumerate() {
        path.push(seg);
        if seg.to_ascii_lowercase().ends_with(".php") && path.is_file() {
            let script_name = format!("/{}", rel[..=i].join("/"));
            let rest = &rel[i + 1..];
            let path_info = if rest.is_empty() {
                String::new()
            } else {
                format!("/{}", rest.join("/"))
            };
            return Target::Php {
                script: path,
                script_name,
                path_info,
            };
        }
    }
    let url = format!("/{}", rel.join("/"));
    if path.is_file() {
        return Target::Static(path);
    }
    if path.is_dir() {
        let index = path.join("index.php");
        if index.is_file() {
            let script_name = format!("{}/index.php", url.trim_end_matches('/'));
            return Target::Php {
                script: index,
                script_name,
                path_info: String::new(),
            };
        }
        if path.join("index.html").is_file() {
            return Target::Static(path.join("index.html"));
        }
    }
    let front = docroot.join("index.php");
    if front.is_file() {
        return Target::Php {
            script: front,
            script_name: "/index.php".into(),
            path_info: String::new(),
        };
    }
    Target::NotFound
}

// ---------------------------------------------------------------- static files

async fn serve_static(req: &Request<Incoming>, file: &Path) -> Response<Body> {
    let Ok(meta) = tokio::fs::metadata(file).await else {
        return text(StatusCode::NOT_FOUND, "Not Found");
    };
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let etag = format!("\"{mtime:x}-{:x}\"", meta.len());
    let not_modified = req
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|t| t.trim() == etag));

    let builder = Response::builder()
        .header(header::ETAG, &etag)
        // Development server: always revalidate, so edits show up on reload.
        .header(header::CACHE_CONTROL, "no-cache");
    if not_modified {
        return builder
            .status(StatusCode::NOT_MODIFIED)
            .body(empty())
            .unwrap_or_else(|_| internal());
    }
    let builder = builder
        .header(header::CONTENT_TYPE, content_type(file))
        .header(header::CONTENT_LENGTH, meta.len());

    let body = if meta.len() <= INLINE_FILE_MAX {
        match tokio::fs::read(file).await {
            Ok(data) => full(data),
            Err(_) => return text(StatusCode::INTERNAL_SERVER_ERROR, "read error"),
        }
    } else {
        let (tx, rx) = mpsc::channel(4);
        let path = file.to_owned();
        tokio::spawn(async move {
            let Ok(mut f) = tokio::fs::File::open(&path).await else {
                return;
            };
            let mut buf = vec![0u8; STREAM_CHUNK];
            while let Ok(n) = f.read(&mut buf).await {
                if n == 0 || tx.send(Bytes::copy_from_slice(&buf[..n])).await.is_err() {
                    break;
                }
            }
        });
        ChannelBody(rx).boxed()
    };
    builder.body(body).unwrap_or_else(|_| internal())
}

fn content_type(file: &Path) -> &'static str {
    let ext = file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "txt" => "text/plain; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "xml" => "application/xml",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "eot" => "application/vnd.ms-fontobject",
        "pdf" => "application/pdf",
        "wasm" => "application/wasm",
        "zip" => "application/zip",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        _ => "application/octet-stream",
    }
}

// ---------------------------------------------------------------- PHP

struct PhpRequest<'a> {
    docroot: &'a Path,
    script: &'a Path,
    script_name: String,
    path_info: String,
    host: &'a str,
    remote: SocketAddr,
    https: bool,
    server_port: u16,
}

async fn run_php(pools: &Arc<Pools>, mut lease: Lease, req: Request<Incoming>, ctx: PhpRequest<'_>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    // PHP needs CONTENT_LENGTH up front; a chunked upload is buffered to learn it.
    let declared = parts
        .headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok()?.parse::<u64>().ok());
    let (content_length, buffered, streamed) = match declared {
        Some(n) => (n, None, Some(body)),
        None => match body.collect().await {
            Ok(b) => {
                let b = b.to_bytes();
                (b.len() as u64, Some(b), None)
            }
            Err(_) => {
                pools.release(lease, None).await;
                return text(StatusCode::BAD_REQUEST, "bad request body");
            }
        },
    };

    let params = php_params(&parts, &ctx, content_length);
    let is_head = parts.method == http::Method::HEAD;
    let streamed = streamed.filter(|_| content_length > 0);
    let reused = lease.conn.is_some();
    let mut conn = match lease.conn.take() {
        Some(c) => c,
        None => match TcpStream::connect((Ipv4Addr::LOCALHOST, lease.port)).await {
            Ok(c) => c,
            Err(e) => {
                pools.release(lease, None).await;
                return text(StatusCode::BAD_GATEWAY, &format!("PHPev: cannot reach php-cgi: {e}"));
            }
        },
    };
    let _ = conn.set_nodelay(true);

    // A kept-alive connection can be stale (worker restarted). Requests without a
    // body can simply be replayed on a fresh connection.
    let mut sent = send_request(&mut conn, &params, buffered.as_ref(), streamed).await;
    let mut reader = BufReader::new(conn);
    let mut first = match &sent {
        Ok(()) => fastcgi::read_event(&mut reader).await,
        Err(e) => Err(io::Error::new(e.kind(), e.to_string())),
    };
    if first.is_err() && reused && content_length == 0 {
        match TcpStream::connect((Ipv4Addr::LOCALHOST, lease.port)).await {
            Ok(mut fresh) => {
                let _ = fresh.set_nodelay(true);
                sent = send_request(&mut fresh, &params, None, None).await;
                reader = BufReader::new(fresh);
                first = match sent {
                    Ok(()) => fastcgi::read_event(&mut reader).await,
                    Err(e) => Err(e),
                };
            }
            Err(e) => first = Err(e),
        }
    }

    // Collect STDOUT until the CGI head is complete.
    let mut head_buf = Vec::new();
    let mut event = first;
    let (head, body_start, ended) = loop {
        match event {
            Ok(Event::Stdout(data)) => {
                head_buf.extend_from_slice(&data);
                if let Some((head, at)) = fastcgi::parse_head(&head_buf) {
                    break (head, at, false);
                }
            }
            Ok(Event::Stderr(data)) => log_warn!("php stderr: {}", String::from_utf8_lossy(&data).trim()),
            Ok(Event::End { .. }) => {
                if head_buf.is_empty() {
                    pools.release(lease, Some(reader.into_inner())).await;
                    return text(StatusCode::BAD_GATEWAY, "PHPev: php-cgi returned no response");
                }
                // Output without a header terminator: treat it all as body.
                break (
                    fastcgi::Head {
                        status: 200,
                        headers: Vec::new(),
                    },
                    0,
                    true,
                );
            }
            Err(e) => {
                pools.release(lease, None).await;
                return text(
                    StatusCode::BAD_GATEWAY,
                    &format!("PHPev: php-cgi connection failed: {e}"),
                );
            }
        }
        event = fastcgi::read_event(&mut reader).await;
    };

    // Keep reading while PHP keeps producing without pausing: most responses
    // finish here and go out with Content-Length, no channel, no extra task.
    let mut body_buf = head_buf.split_off(body_start);
    let mut ended = ended;
    while !ended && body_buf.len() <= INLINE_RESPONSE_MAX {
        // fill_buf is cancel-safe (consumes nothing); read_event is not, so
        // only the wait for the *next* record is bounded by the timeout.
        match tokio::time::timeout(STREAM_AFTER_PAUSE, reader.fill_buf()).await {
            Err(_) => break,
            Ok(Err(e)) => {
                pools.release(lease, None).await;
                return text(
                    StatusCode::BAD_GATEWAY,
                    &format!("PHPev: php-cgi connection failed: {e}"),
                );
            }
            Ok(Ok(_)) => {}
        }
        match fastcgi::read_event(&mut reader).await {
            Ok(Event::Stdout(data)) => body_buf.extend_from_slice(&data),
            Ok(Event::Stderr(data)) => log_warn!("php stderr: {}", String::from_utf8_lossy(&data).trim()),
            Ok(Event::End { .. }) => ended = true,
            Err(e) => {
                pools.release(lease, None).await;
                return text(
                    StatusCode::BAD_GATEWAY,
                    &format!("PHPev: php-cgi connection failed: {e}"),
                );
            }
        }
    }

    let mut builder = Response::builder().status(StatusCode::from_u16(head.status).unwrap_or(StatusCode::OK));
    for (name, value) in &head.headers {
        let Ok(name) = HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        if name == header::CONNECTION || name == header::TRANSFER_ENCODING {
            continue;
        }
        if let Ok(value) = HeaderValue::from_str(value) {
            builder = builder.header(name, value);
        }
    }

    if ended {
        pools.release(lease, Some(reader.into_inner())).await;
        let has_length = head
            .headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case("content-length"));
        if !has_length && !is_head {
            builder = builder.header(header::CONTENT_LENGTH, body_buf.len());
        }
        return builder.body(full(body_buf)).unwrap_or_else(|_| internal());
    }

    let (tx, rx) = mpsc::channel::<Bytes>(8);
    let first_chunk = Bytes::from(body_buf);
    let pools = pools.clone();
    tokio::spawn(async move {
        if !first_chunk.is_empty() && tx.send(first_chunk).await.is_err() {
            // Client went away: drop the connection; PHP aborts at its next output.
            pools.release(lease, None).await;
            return;
        }
        loop {
            match fastcgi::read_event(&mut reader).await {
                Ok(Event::Stdout(data)) => {
                    if tx.send(Bytes::from(data)).await.is_err() {
                        pools.release(lease, None).await;
                        return;
                    }
                }
                Ok(Event::Stderr(data)) => log_warn!("php stderr: {}", String::from_utf8_lossy(&data).trim()),
                Ok(Event::End { .. }) => {
                    pools.release(lease, Some(reader.into_inner())).await;
                    return;
                }
                Err(_) => {
                    pools.release(lease, None).await;
                    return;
                }
            }
        }
    });
    builder.body(ChannelBody(rx).boxed()).unwrap_or_else(|_| internal())
}

async fn send_request(
    conn: &mut TcpStream,
    params: &Params,
    buffered: Option<&Bytes>,
    streamed: Option<Incoming>,
) -> io::Result<()> {
    let no_body = buffered.is_none_or(|b| b.is_empty()) && streamed.is_none();
    fastcgi::begin(conn, params, no_body).await?;
    if no_body {
        return conn.flush().await;
    }
    if let Some(data) = buffered {
        fastcgi::stdin(conn, data).await?;
    }
    if let Some(mut body) = streamed {
        while let Some(frame) = body.frame().await {
            let frame = frame.map_err(io::Error::other)?;
            if let Some(data) = frame.data_ref() {
                fastcgi::stdin(conn, data).await?;
            }
        }
    }
    fastcgi::stdin(conn, &[]).await?;
    conn.flush().await
}

fn php_params(parts: &http::request::Parts, ctx: &PhpRequest<'_>, content_length: u64) -> Params {
    let mut p = Params::default();
    let uri = &parts.uri;
    let request_uri = uri.path_and_query().map_or("/", |pq| pq.as_str());
    let docroot = ctx.docroot.to_string_lossy();
    let script = ctx.script.to_string_lossy();
    p.add("GATEWAY_INTERFACE", "CGI/1.1")
        .add("SERVER_SOFTWARE", concat!("PHPev/", env!("CARGO_PKG_VERSION")))
        .add("SERVER_PROTOCOL", format!("{:?}", parts.version))
        .add("SERVER_NAME", ctx.host)
        .add("SERVER_ADDR", "127.0.0.1")
        .add("SERVER_PORT", ctx.server_port.to_string())
        .add("REQUEST_SCHEME", if ctx.https { "https" } else { "http" })
        .add("REQUEST_METHOD", parts.method.as_str())
        .add("REQUEST_URI", request_uri)
        .add("QUERY_STRING", uri.query().unwrap_or(""))
        .add("DOCUMENT_ROOT", docroot.as_bytes())
        .add("DOCUMENT_URI", &ctx.script_name)
        .add("SCRIPT_NAME", &ctx.script_name)
        .add("SCRIPT_FILENAME", script.as_bytes())
        .add("PHP_SELF", format!("{}{}", ctx.script_name, ctx.path_info))
        .add("REMOTE_ADDR", ctx.remote.ip().to_string())
        .add("REMOTE_PORT", ctx.remote.port().to_string())
        // php-cgi refuses to run without it (cgi.force_redirect).
        .add("REDIRECT_STATUS", "200")
        .add("HTTPS", if ctx.https { "on" } else { "" })
        .add(
            "CONTENT_LENGTH",
            if content_length > 0 {
                content_length.to_string()
            } else {
                String::new()
            },
        );
    if !ctx.path_info.is_empty() {
        p.add("PATH_INFO", &ctx.path_info);
    }
    if let Some(ct) = parts.headers.get(header::CONTENT_TYPE) {
        p.add("CONTENT_TYPE", ct.as_bytes());
    }
    for (name, value) in &parts.headers {
        if name == header::CONTENT_TYPE || name == header::CONTENT_LENGTH {
            continue;
        }
        // HTTP_PROXY would let a request header poison outgoing proxies ("httpoxy").
        if name.as_str().eq_ignore_ascii_case("proxy") {
            continue;
        }
        let key = format!("HTTP_{}", name.as_str().to_ascii_uppercase().replace('-', "_"));
        p.add(&key, value.as_bytes());
    }
    p
}

// ---------------------------------------------------------------- dashboard of sites

/// Site list at http://localhost, and the 404 page for unknown hosts.
fn dashboard(status: StatusCode, sites: &[Site], config: &Config, unknown: Option<&str>) -> Response<Body> {
    let mut html = String::from(
        "<!doctype html><meta charset=utf-8><title>PHPev</title>\
         <style>body{font:15px system-ui,sans-serif;max-width:720px;margin:40px auto;padding:0 16px}\
         td{padding:4px 12px 4px 0}code{color:#666}</style><h1>PHPev</h1>",
    );
    if let Some(host) = unknown {
        html += &format!("<p>No site is configured for <b>{}</b>.</p>", escape(host));
    }
    if sites.is_empty() {
        html += "<p>No sites yet. Run <code>phpev park C:\\code</code> or <code>phpev link</code> in a project.</p>";
    } else {
        html += "<table>";
        for s in sites {
            let scheme = if s.secure { "https" } else { "http" };
            let url = format!("{scheme}://{}", s.host(&config.tld));
            let kind = match s.kind {
                sites::Kind::Linked => "linked",
                sites::Kind::Parked => "parked",
            };
            html += &format!(
                "<tr><td><a href=\"{u}\">{u}</a></td><td>{kind}</td><td><code>{}</code></td></tr>",
                escape(&s.path.to_string_lossy()),
                u = escape(&url)
            );
        }
        html += "</table>";
    }
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(full(html))
        .unwrap_or_else(|_| internal())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segs(p: &str) -> Option<Vec<String>> {
        sanitize(p)
    }

    #[test]
    fn sanitize_rejects_escapes_and_hidden_files() {
        assert_eq!(segs("/css/app.css").unwrap(), ["css", "app.css"]);
        assert_eq!(segs("/a%20b/").unwrap(), ["a b"]);
        assert_eq!(segs("/.well-known/acme").unwrap(), [".well-known", "acme"]);
        for bad in [
            "/../x",
            "/%2e%2e/x",
            "/.env",
            "/a/.git/config",
            "/ENV~1",
            "/index.php.",
            "/x%5c..%5cy",
            "/c:/w",
            "/a%00b",
            "/x/.well-known",
            "/bad%zz",
        ] {
            assert!(segs(bad).is_none(), "{bad} should be rejected");
        }
    }

    #[test]
    fn routing() {
        let root = std::env::temp_dir().join(format!("phpev-route-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(r"public\css")).unwrap();
        std::fs::create_dir_all(root.join(r"public\admin")).unwrap();
        std::fs::create_dir_all(root.join(r"public\docs")).unwrap();
        for f in [
            r"public\index.php",
            r"public\css\app.css",
            r"public\info.php",
            r"public\admin\index.php",
            r"public\docs\index.html",
        ] {
            std::fs::write(root.join(f), "").unwrap();
        }
        let doc = root.join("public");
        let r = |p: &str| route(&doc, &sanitize(p).unwrap());

        assert_eq!(r("/css/app.css"), Target::Static(doc.join(r"css\app.css")));
        assert!(matches!(r("/users/42"), Target::Php { script_name, .. } if script_name == "/index.php"));
        assert!(matches!(r("/"), Target::Php { script_name, .. } if script_name == "/index.php"));
        assert!(matches!(r("/info.php/a/b"), Target::Php { script_name, path_info, .. }
            if script_name == "/info.php" && path_info == "/a/b"));
        assert!(matches!(r("/admin/"), Target::Php { script_name, .. } if script_name == "/admin/index.php"));
        assert_eq!(r("/docs"), Target::Static(doc.join(r"docs\index.html")));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
