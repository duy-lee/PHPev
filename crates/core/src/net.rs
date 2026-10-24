//! HTTP downloads with on-the-fly SHA-256 verification.
//!
//! Uses the Windows TLS stack (schannel) so corporate proxies/CAs trusted by
//! Windows work out of the box and no crypto library is compiled in.

use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

/// Progress callback: `(bytes_done, total_bytes_if_known)`.
pub type Progress<'a> = &'a mut dyn FnMut(u64, Option<u64>);

#[derive(Clone)]
pub struct Http {
    agent: ureq::Agent,
}

impl Http {
    pub fn new() -> io::Result<Self> {
        let tls = native_tls::TlsConnector::new().map_err(io::Error::other)?;
        let agent = ureq::AgentBuilder::new()
            .tls_connector(Arc::new(tls))
            .user_agent(concat!("phpev/", env!("CARGO_PKG_VERSION")))
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(60))
            .build();
        Ok(Self { agent })
    }

    fn get(&self, url: &str) -> io::Result<ureq::Response> {
        self.agent.get(url).call().map_err(|e| match e {
            ureq::Error::Status(code, _) => io::Error::other(format!("GET {url}: HTTP {code}")),
            ureq::Error::Transport(t) => io::Error::other(format!("GET {url}: {t}")),
        })
    }

    pub fn get_json<T: DeserializeOwned>(&self, url: &str) -> io::Result<T> {
        let reader = self.get(url)?.into_reader();
        serde_json::from_reader(reader).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{url}: {e}")))
    }

    pub fn get_text(&self, url: &str) -> io::Result<String> {
        self.get(url)?.into_string()
    }

    /// Streams `url` into `dest`, hashing while writing. The file only appears at
    /// `dest` once complete and (if `expected_sha256` is given) verified.
    pub fn download(
        &self,
        url: &str,
        dest: &Path,
        expected_sha256: Option<&str>,
        progress: Progress<'_>,
    ) -> io::Result<()> {
        let response = self.get(url)?;
        let total = response.header("Content-Length").and_then(|v| v.parse().ok());
        let mut reader = response.into_reader();

        let part = with_suffix(dest, ".part");
        let result = (|| {
            let mut out = BufWriter::with_capacity(1 << 20, File::create(&part)?);
            let mut hasher = Sha256::new();
            let mut buf = vec![0u8; 256 * 1024];
            let mut done = 0u64;
            progress(0, total);
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                out.write_all(&buf[..n])?;
                done += n as u64;
                progress(done, total);
            }
            out.flush()?;
            drop(out);

            if let Some(t) = total.filter(|&t| t != done) {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("{url}: got {done} of {t} bytes"),
                ));
            }
            let actual = hex(&hasher.finalize());
            if let Some(expected) = expected_sha256 {
                if !actual.eq_ignore_ascii_case(expected.trim()) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{url}: SHA-256 mismatch (expected {expected}, got {actual})"),
                    ));
                }
            }
            std::fs::rename(&part, dest)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&part);
        }
        result
    }
}

/// Uses `dest` if it already exists with the expected hash (an earlier,
/// interrupted install); otherwise (re)downloads it.
pub fn fetch_verified(http: &Http, url: &str, dest: &Path, sha256: &str, progress: Progress<'_>) -> io::Result<()> {
    if dest.is_file() {
        if sha256_file(dest)?.eq_ignore_ascii_case(sha256.trim()) {
            return Ok(());
        }
        std::fs::remove_file(dest)?;
    }
    http.download(url, dest, Some(sha256), progress)
}

pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher)?;
    Ok(hex(&hasher.finalize()))
}

/// Upgrades `http://` to `https://` (the MariaDB API hands out plain-HTTP links).
pub fn force_https(url: &str) -> String {
    match url.strip_prefix("http://") {
        Some(rest) => format!("https://{rest}"),
        None => url.to_owned(),
    }
}

/// First whitespace-separated token of a `<hash>  <file>` checksum file.
pub fn parse_checksum_file(text: &str) -> Option<&str> {
    text.split_whitespace()
        .next()
        .filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_upgrade() {
        assert_eq!(force_https("http://a.org/x.zip"), "https://a.org/x.zip");
        assert_eq!(force_https("https://a.org/x.zip"), "https://a.org/x.zip");
    }

    #[test]
    fn checksum_file_parsing() {
        let h = "93bd8a815a52156606fb906d0573437908c61f96e2cc52c03a51e36a41bf5236";
        assert_eq!(parse_checksum_file(&format!("{h}  composer.phar\n")), Some(h));
        assert_eq!(parse_checksum_file(h), Some(h));
        assert_eq!(parse_checksum_file("<html>"), None);
    }
}
