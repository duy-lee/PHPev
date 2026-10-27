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
    /// Detected from the project's files.
    pub framework: crate::project::Framework,
    /// PHP requests in the last 10 minutes (only filled by `GET /sites`).
    #[serde(default)]
    pub requests_10m: u64,
}

/// Everything the site page shows (`GET /sites/<name>`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SiteDetail {
    pub site: SiteInfo,
    /// Where the site's PHP version comes from: `pinned`, `version-file`, `default` or `newest`.
    pub php_source: String,
    /// Installed PHP lines (`8.4`, `7.1`) the site can be pinned to.
    pub php_lines: Vec<String>,
    /// php-cgi workers of the site's PHP version (shared with other sites on it).
    pub workers: usize,
    pub workers_memory_kb: u64,
    /// The PHPev certificate authority is trusted by Windows.
    pub ca_trusted: bool,
    /// Composer version the `composer` shim runs for this site.
    pub composer: Option<String>,
    /// Connections from `.env` or `wp-config.php`.
    pub databases: Vec<SiteDb>,
    pub checks: Vec<crate::project::Check>,
    /// The site's own logs, then the PHP error log of its version.
    pub logs: Vec<LogSource>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SiteDb {
    #[serde(flatten)]
    pub settings: crate::project::DbSettings,
    /// On PHPev's MariaDB, with a database name: its tables can be shown.
    pub local: bool,
}

/// One local database of a site (`GET /sites/<name>/database?name=<connection>`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SiteDatabase {
    /// The connection (`.env` prefix), as in `SiteDb`.
    pub name: String,
    pub database: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub mariadb_running: bool,
    /// Version of the running server.
    pub mariadb_version: Option<String>,
    /// `None` when MariaDB is not running.
    pub exists: Option<bool>,
    pub tables: Vec<crate::mariadb::TableInfo>,
    pub bytes: u64,
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
    Editor,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenRequest {
    pub target: OpenTarget,
    /// With `editor`: the editor to use, remembered for next time; without,
    /// the remembered one.
    #[serde(default)]
    pub editor: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditorChoice {
    pub id: String,
    pub name: String,
}

/// `GET /editors`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditorsInfo {
    /// Found on this computer.
    pub editors: Vec<EditorChoice>,
    /// The remembered choice, if it is still installed.
    pub chosen: Option<String>,
    /// Every editor PHPev looks for.
    pub supported: Vec<String>,
}

/// `POST /sites/<name>/fix/<check>`: done at once (a message) or as a job.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FixResult {
    pub message: Option<String>,
    pub job: Option<u64>,
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

/// Import a `.sql` file (picked with `.../database/import/pick`) into a site's database.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DbImportRequest {
    /// The connection (`.env` prefix), as in `SiteDb`.
    pub name: String,
    pub path: String,
    /// Drop and recreate the database first.
    pub empty_first: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DbExportRequest {
    pub name: String,
}

/// A job, or none when the user cancelled the file dialog before it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MaybeJob {
    pub job: Option<u64>,
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
    /// Stopped by the user (`POST /jobs/<id>/cancel`).
    Cancelled,
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
    /// The job can be stopped with `POST /jobs/<id>/cancel`.
    #[serde(default)]
    pub cancellable: bool,
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

/// Everything the Settings page shows (`GET /settings`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingsInfo {
    pub version: String,
    /// Starts at Windows sign-in.
    pub autostart: bool,
    pub tray: bool,
    /// `light`, `dark` or `system`.
    pub theme: String,
    /// Look for a newer release once a day.
    pub check_updates: bool,
    pub tld: String,
    pub parked: Vec<String>,
    pub ports: crate::config::Ports,
    /// The local certificate authority exists (HTTPS was turned on once).
    pub ca_exists: bool,
    pub ca_trusted: bool,
    pub secure_sites: Vec<String>,
    /// The PHPev home is excluded from Defender scanning.
    pub defender_excluded: bool,
    pub checks: Vec<Check>,
    pub home: String,
    pub config_file: String,
}

/// `PUT /settings/general`: only the fields sent change.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GeneralSettings {
    pub autostart: Option<bool>,
    pub tray: Option<bool>,
    pub theme: Option<String>,
    /// Id of an installed editor (`GET /editors`).
    pub editor: Option<String>,
    pub check_updates: Option<bool>,
}

/// The saved result of the daily release check.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub current: String,
    /// Newest published release, if one was found.
    pub latest: Option<String>,
    /// A release to offer: newer, not skipped and the check is on.
    pub available: Option<String>,
    /// Release page of `latest`.
    pub url: Option<String>,
    /// Unix time of the last check; 0 = never.
    pub checked_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TldRequest {
    pub tld: String,
}

/// Result of a change that may restart PHPev (ports, TLD).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingsApplied {
    pub message: String,
    /// PHPev restarts to apply it; the dashboard reopens by itself.
    pub restarting: bool,
}

/// One folder of the Storage section (`GET /settings/storage`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StorageItem {
    /// `php`, `mariadb`, `databases`, `composer`, `downloads`, `logs`.
    pub id: String,
    pub path: String,
    pub bytes: u64,
    /// "Clean up" can empty it (download cache, old logs).
    pub clearable: bool,
}
