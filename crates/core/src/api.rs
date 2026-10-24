//! Dashboard API v1 types. They live in core so that any
//! client — the web dashboard, a future Slint app or TUI — shares the exact
//! wire format. Breaking changes go to a new `/api/v2`.

use serde::{Deserialize, Serialize};

use crate::doctor::Check;
use crate::ipc::Status;

pub const PREFIX: &str = "/api/v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Overview {
    pub version: String,
    pub status: Status,
    pub checks: Vec<Check>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SiteInfo {
    pub name: String,
    pub url: String,
    /// `linked` or `parked`.
    pub kind: String,
    pub path: String,
    /// Version pinned in config (`phpev link --php`), if any.
    pub php_pinned: Option<String>,
    /// Installed PHP that requests to this site run on, or why none applies.
    pub php: Result<String, String>,
    pub secure: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkRequest {
    pub path: String,
    pub name: Option<String>,
    pub php: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecureRequest {
    pub secure: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PhpPinRequest {
    /// `None` removes the pin.
    pub version: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpenTarget {
    Browser,
    Folder,
    Terminal,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenRequest {
    pub target: OpenTarget,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PhpVersionInfo {
    pub version: String,
    /// The configured default resolves to this version.
    pub default: bool,
    /// Running php-cgi workers.
    pub workers: usize,
    /// Linked sites that lose their PHP if this version is removed.
    pub pinned_by: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PhpInfo {
    /// Configured default (a line like `8.4`), if set.
    pub default: Option<String>,
    pub installed: Vec<PhpVersionInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AvailablePhp {
    pub line: String,
    pub version: String,
    pub installed: bool,
    /// An end-of-life line, downloaded from the archive.
    pub archived: bool,
}

/// A folder on this computer, e.g. a PHP build to add.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PathRequest {
    pub path: String,
}

/// The folder chosen in the Windows folder dialog; `None` if cancelled.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PickedFolder {
    pub path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionRequest {
    pub version: String,
}

/// Removing a MariaDB version.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoveMariaDbRequest {
    /// Also delete the databases of its major when no other release of it is left.
    pub delete_data: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionRequest {
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MariaDbInfo {
    pub installed: Vec<String>,
    /// Active major from config (`11.4`).
    pub active: Option<String>,
    pub port: u16,
    pub running: Option<crate::ipc::Service>,
    pub data_dir: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AvailableMariaDb {
    pub major: String,
    pub lts: bool,
    pub eol: Option<String>,
    pub eol_passed: bool,
    pub installed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobStarted {
    pub job: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Running,
    Ok,
    Error,
}

/// A long operation (install) the dashboard follows over SSE.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobInfo {
    pub id: u64,
    pub title: String,
    pub status: JobStatus,
    pub lines: Vec<String>,
    /// Current download: (label, bytes done, total bytes).
    pub progress: Option<(String, u64, Option<u64>)>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: String,
}

// ---------------------------------------------------------------- M9: metrics and logs

/// Request statistics over a sliding window.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RequestStats {
    pub window_secs: u32,
    /// Unix time (ms) when the stats were computed; lets clients check lag.
    pub generated_ms: u64,
    pub total: u64,
    /// Requests per second, oldest first, one entry per second of the window.
    pub per_second: Vec<u32>,
    /// Median and 95th percentile latency per second (ms), `None` when idle.
    pub p50_per_second: Vec<Option<f64>>,
    pub p95_per_second: Vec<Option<f64>>,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    /// Latency histogram: (upper bound label, count).
    pub histogram: Vec<(String, u32)>,
    pub status: StatusCounts,
    pub slowest: Vec<RouteStats>,
    /// Hosts seen in the buffer (for the site filter).
    pub hosts: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StatusCounts {
    pub s2xx: u32,
    pub s3xx: u32,
    pub s4xx: u32,
    pub s5xx: u32,
}

/// A route is a normalized path: `/users/42` → `/users/{n}`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouteStats {
    pub host: String,
    pub method: String,
    pub route: String,
    pub count: u32,
    pub avg_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcessSample {
    /// `daemon`, `mariadb`, `php-cgi 8.4.27`.
    pub name: String,
    pub pid: u32,
    /// Share of one CPU core (100 = one full core) since the previous sample.
    pub cpu_percent: f64,
    /// Private working set (KiB).
    pub memory_kb: Option<u64>,
}

/// One push of the live performance stream (every second).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiveMetrics {
    pub requests: RequestStats,
    pub processes: Vec<ProcessSample>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogSource {
    /// `daemon`, `php:<version>`, `mariadb:<major>`, `site:<name>:<file>`.
    pub id: String,
    pub label: String,
    pub path: String,
}

/// New log lines; `reset` means the file was rotated or truncated.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogChunk {
    pub lines: Vec<String>,
    pub reset: bool,
}
