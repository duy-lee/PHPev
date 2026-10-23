//! Log viewing for the dashboard (DESIGN_SPEC §14.4): a fixed list of sources
//! (PHPev, PHP error logs, MariaDB, and each site's Laravel `storage/logs`),
//! resolved by id — never by a client-supplied path.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use phpev_core::api::LogSource;
use phpev_core::config::Config;
use phpev_core::layout::Layout;
use phpev_core::sites;

/// Longest line sent (longer ones are cut, e.g. minified stack traces).
const MAX_LINE: usize = 8 * 1024;
/// Bytes read per poll at most, so a huge burst cannot stall the stream.
const MAX_CHUNK: u64 = 512 * 1024;

/// Every log a viewer may open.
pub fn sources(layout: &Layout) -> io::Result<Vec<LogSource>> {
    let mut out = Vec::new();
    let logs = layout.logs_dir();
    let push = |out: &mut Vec<LogSource>, id: String, label: String, path: PathBuf| {
        out.push(LogSource {
            id,
            label,
            path: path.display().to_string(),
        })
    };
    push(
        &mut out,
        "daemon".into(),
        "PHPev daemon".into(),
        logs.join("daemon.log"),
    );

    let mut names: Vec<String> = std::fs::read_dir(&logs)?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    for name in &names {
        if let Some(v) = name.strip_prefix("php-").and_then(|n| n.strip_suffix(".log")) {
            push(&mut out, format!("php:{v}"), format!("PHP {v} errors"), logs.join(name));
        } else if let Some(m) = name.strip_prefix("mariadb-").and_then(|n| n.strip_suffix(".err")) {
            push(
                &mut out,
                format!("mariadb:{m}"),
                format!("MariaDB {m}"),
                logs.join(name),
            );
        }
    }

    let config = Config::load(&layout.config_file())?;
    for site in sites::all(&config) {
        let dir = site.path.join(r"storage\logs");
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<String> = entries
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| is_log_name(n))
            .collect();
        files.sort();
        for f in files {
            push(
                &mut out,
                format!("site:{}:{f}", site.name),
                format!("{} · {f}", site.name),
                dir.join(&f),
            );
        }
    }
    Ok(out)
}

/// `laravel.log`, `laravel-2026-10-23.log`: a plain file name ending in .log.
fn is_log_name(n: &str) -> bool {
    n.ends_with(".log") && !n.starts_with('.') && n.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// Path for a source id, looked up in the current list (so ids cannot smuggle paths).
pub fn resolve(layout: &Layout, id: &str) -> io::Result<PathBuf> {
    sources(layout)?
        .into_iter()
        .find(|s| s.id == id)
        .map(|s| PathBuf::from(s.path))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no log `{id}`")))
}

/// The last `n` lines of `path` and the offset after them.
pub fn tail(path: &Path, n: usize) -> io::Result<(Vec<String>, u64)> {
    let mut f = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
        Err(e) => return Err(e),
    };
    let len = f.metadata()?.len();
    // Read backwards in blocks until enough newlines are found.
    let mut start = len;
    let mut buf: Vec<u8> = Vec::new();
    while start > 0 && buf.iter().filter(|&&b| b == b'\n').count() <= n {
        let step = start.min(64 * 1024);
        start -= step;
        let mut block = vec![0u8; step as usize];
        f.seek(SeekFrom::Start(start))?;
        f.read_exact(&mut block)?;
        block.extend_from_slice(&buf);
        buf = block;
    }
    let mut lines = split_lines(&buf);
    if lines.len() > n {
        lines.drain(..lines.len() - n);
    }
    Ok((lines, len))
}

/// Lines appended since `offset`. Returns (lines, new offset, reset) where
/// `reset` means the file shrank (rotated/truncated) and was re-read from 0.
pub fn read_new(path: &Path, offset: u64) -> io::Result<(Vec<String>, u64, bool)> {
    let mut f = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((Vec::new(), 0, offset > 0)),
        Err(e) => return Err(e),
    };
    let len = f.metadata()?.len();
    let (from, reset) = if len < offset { (0, true) } else { (offset, false) };
    if len == from {
        return Ok((Vec::new(), from, reset));
    }
    let to = len.min(from + MAX_CHUNK);
    let mut buf = vec![0u8; (to - from) as usize];
    f.seek(SeekFrom::Start(from))?;
    f.read_exact(&mut buf)?;
    // Only complete lines; a partial last line is picked up next time.
    let keep = if to == len {
        buf.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1)
    } else {
        buf.len()
    };
    buf.truncate(keep);
    Ok((split_lines(&buf), from + keep as u64, reset))
}

fn split_lines(buf: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(buf)
        .lines()
        .map(|l| {
            if l.len() > MAX_LINE {
                format!("{}…", &l[..l.floor_char_boundary(MAX_LINE)])
            } else {
                l.to_owned()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn tail_and_follow_with_rotation() {
        let dir = std::env::temp_dir().join(format!("phpev-logs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.log");
        let mut f = File::create(&file).unwrap();
        for i in 0..1000 {
            writeln!(f, "line {i}").unwrap();
        }
        drop(f);

        let (lines, offset) = tail(&file, 3).unwrap();
        assert_eq!(lines, ["line 997", "line 998", "line 999"]);

        let mut f = std::fs::OpenOptions::new().append(true).open(&file).unwrap();
        write!(f, "new 1\npartial").unwrap();
        let (new, offset2, reset) = read_new(&file, offset).unwrap();
        assert_eq!((new, reset), (vec!["new 1".to_owned()], false));
        writeln!(f, " done").unwrap();
        drop(f);
        let (new, offset3, _) = read_new(&file, offset2).unwrap();
        assert_eq!(new, ["partial done"]);

        std::fs::write(&file, "after rotate\n").unwrap();
        let (new, _, reset) = read_new(&file, offset3).unwrap();
        assert_eq!((new, reset), (vec!["after rotate".to_owned()], true));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn log_names() {
        assert!(is_log_name("laravel.log") && is_log_name("laravel-2026-10-23.log"));
        assert!(!is_log_name("..\\x.log") && !is_log_name(".env.log") && !is_log_name("a.txt"));
    }
}
