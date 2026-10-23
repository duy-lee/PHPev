//! CLI ↔ daemon protocol: one JSON request line, one JSON response line, over
//! a per-user named pipe (DESIGN_SPEC §1).
//!
//! Only *actions* go through the daemon. Config changes are written directly
//! with `config::update` (cross-process lock); the daemon re-reads config.toml
//! for every action, so it never works from a stale copy.

use std::fs::OpenOptions;
use std::io::{self, BufRead, BufReader, Write};

use serde::{Deserialize, Serialize};
use windows_sys::Win32::Foundation::ERROR_PIPE_BUSY;
use windows_sys::Win32::System::Pipes::WaitNamedPipeW;

use crate::wide::to_wide;

/// `\\.\pipe\phpev-<user>`: one daemon per user session.
pub fn pipe_name() -> String {
    let user: String = std::env::var("USERNAME")
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!(r"\\.\pipe\phpev-{user}")
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub enum Request {
    Ping,
    Status,
    /// Stop every service and exit the daemon.
    Shutdown,
    DbStart,
    DbStop,
    /// Switch the active MariaDB major (restarts it if it was running).
    DbUse {
        major: String,
    },
    /// Start the php-cgi pool of a version now instead of on first request.
    PhpWarm {
        version: Option<String>,
    },
    /// Stop the php-cgi pool of an installed version (e.g. after php.ini
    /// changed); it starts again on the next request.
    PhpRestart {
        version: String,
    },
    /// Start the HTTPS listener now (after `phpev secure` created the CA).
    EnableHttps,
    /// The dashboard login URL, including this daemon run's token.
    DashboardUrl,
    /// Show or hide the tray icon (after `phpev tray on|off` saved it).
    SetTray {
        enabled: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub enum Response {
    Ok { message: String },
    Status(Box<Status>),
    Error { message: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub pid: u32,
    pub uptime_secs: u64,
    pub memory_kb: u64,
    pub mariadb: Option<Service>,
    pub php_pools: Vec<Pool>,
    #[serde(default)]
    pub router: RouterStatus,
    /// Dashboard address (without the login token), or why it is not running.
    #[serde(default = "dashboard_unknown")]
    pub dashboard: Result<String, String>,
}

fn dashboard_unknown() -> Result<String, String> {
    Err("unknown".into())
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouterStatus {
    /// Bound addresses, e.g. `127.0.0.1:80`.
    pub listening: Vec<String>,
    /// Why the router is not serving (e.g. port 80 taken), if it is not.
    pub error: Option<String>,
    pub tld: String,
    pub port: u16,
    /// HTTPS listener, once a site has been secured.
    #[serde(default)]
    pub https_listening: Vec<String>,
    #[serde(default)]
    pub https_error: Option<String>,
    #[serde(default)]
    pub https_port: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Service {
    pub version: String,
    pub port: u16,
    pub pid: Option<u32>,
    pub memory_kb: Option<u64>,
    /// `starting`, `running`, `restarting`, `stopping`.
    pub state: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pool {
    pub version: String,
    pub workers: Vec<Worker>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Worker {
    pub pid: u32,
    pub port: u16,
    pub memory_kb: Option<u64>,
}

/// Sends one request to the running daemon. `ErrorKind::NotFound` means no daemon.
pub fn call(request: &Request) -> io::Result<Response> {
    let name = pipe_name();
    let pipe = loop {
        match OpenOptions::new().read(true).write(true).open(&name) {
            Ok(p) => break p,
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                // All instances busy: wait for one to free up.
                if unsafe { WaitNamedPipeW(to_wide(&name).as_ptr(), 5000) } == 0 {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "the PHPev daemon is busy"));
                }
            }
            Err(e) => return Err(e),
        }
    };

    let mut line = serde_json::to_string(request).map_err(io::Error::other)?;
    line.push('\n');
    (&pipe).write_all(line.as_bytes())?;

    let mut reply = String::new();
    BufReader::new(&pipe).read_line(&mut reply)?;
    if reply.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the PHPev daemon closed the connection",
        ));
    }
    serde_json::from_str(&reply).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn is_running() -> bool {
    matches!(call(&Request::Ping), Ok(Response::Ok { .. }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_is_stable() {
        let r = Request::DbUse { major: "11.4".into() };
        assert_eq!(serde_json::to_string(&r).unwrap(), r#"{"cmd":"db-use","major":"11.4"}"#);
        let ok = Response::Ok { message: "x".into() };
        assert_eq!(serde_json::to_string(&ok).unwrap(), r#"{"result":"ok","message":"x"}"#);
        let back: Request = serde_json::from_str(r#"{"cmd":"php-warm","version":null}"#).unwrap();
        assert_eq!(back, Request::PhpWarm { version: None });
    }

    #[test]
    fn pipe_name_is_sanitized() {
        assert!(pipe_name().starts_with(r"\\.\pipe\phpev-"));
        assert!(!pipe_name()[r"\\.\pipe\".len()..].contains(['\\', ' ']));
    }
}
