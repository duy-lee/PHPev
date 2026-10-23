//! Dashboard API v1 types (DESIGN_SPEC §14.1). They live in core so that any
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionRequest {
    pub version: String,
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
