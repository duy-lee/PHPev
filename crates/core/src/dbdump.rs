//! Importing a `.sql` file into a database and exporting one, with progress
//! and cancellation (for the dashboard's jobs).

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

use crate::layout::Layout;
use crate::mariadb::{self, is_safe_db_name, version_dir};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Large enough for dumps with big rows or long multi-row INSERTs; the server
/// is started with the same limit.
pub const MAX_PACKET: &str = "--max-allowed-packet=1G";
const CHUNK: usize = 256 * 1024;

fn client(layout: &Layout, version: &str, exe: &str, port: u16) -> Command {
    let mut cmd = Command::new(version_dir(layout, version).join("bin").join(exe));
    cmd.args([
        "--no-defaults",
        "--user=root",
        "--host=127.0.0.1",
        &format!("--port={port}"),
        MAX_PACKET,
        "--default-character-set=utf8mb4",
    ])
    .creation_flags(CREATE_NO_WINDOW);
    cmd
}

fn check_name(db: &str) -> io::Result<()> {
    if is_safe_db_name(db) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported database name `{db}`"),
        ))
    }
}

fn cancelled() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "cancelled")
}

/// Runs `work` while another thread ends process `pid` as soon as `cancel`
/// is set: a write blocked on the process's input must not delay it.
fn with_cancel<T>(pid: u32, cancel: &AtomicBool, work: impl FnOnce() -> T) -> T {
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                if cancel.load(Ordering::SeqCst) {
                    unsafe {
                        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
                        if !handle.is_null() {
                            TerminateProcess(handle, 1);
                            CloseHandle(handle);
                        }
                    }
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        let out = work();
        done.store(true, Ordering::SeqCst);
        out
    })
}

fn stderr_text(child: &mut Child) -> std::thread::JoinHandle<String> {
    let mut stderr = child.stderr.take();
    std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(s) = stderr.as_mut() {
            let _ = s.read_to_string(&mut text);
        }
        text
    })
}

/// The useful part of the client's error output.
fn client_error(text: &str, fallback: &str) -> io::Error {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("WARNING:"))
        .take(3)
        .collect();
    io::Error::other(if lines.is_empty() {
        fallback.to_owned()
    } else {
        lines.join(" ")
    })
}

/// Runs `file` into database `db` (created if missing; with `empty_first`,
/// dropped and recreated first). `progress(done, total)` counts bytes read.
#[allow(clippy::too_many_arguments)]
pub fn import(
    layout: &Layout,
    version: &str,
    port: u16,
    db: &str,
    file: &Path,
    empty_first: bool,
    progress: &mut dyn FnMut(u64, Option<u64>),
    cancel: &AtomicBool,
) -> io::Result<()> {
    check_name(db)?;
    let total = std::fs::metadata(file)?.len();
    let create = format!("CREATE DATABASE IF NOT EXISTS `{db}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci");
    let sql = if empty_first {
        format!("DROP DATABASE IF EXISTS `{db}`; {create}")
    } else {
        create
    };
    mariadb::exec_sql(layout, version, port, &sql)?;

    let mut child = client(layout, version, "mariadb.exe", port)
        .arg(db)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let errors = stderr_text(&mut child);
    let status = with_cancel(child.id(), cancel, || {
        let mut input = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("no input to the client"))?;
        let mut source = File::open(file)?;
        let mut buf = vec![0u8; CHUNK];
        let mut sent = 0u64;
        loop {
            let n = source.read(&mut buf)?;
            if n == 0 {
                break;
            }
            // A failed write means the client stopped (an SQL error, or cancelled):
            // its exit status and output tell which.
            if input.write_all(&buf[..n]).is_err() {
                break;
            }
            sent += n as u64;
            progress(sent, Some(total));
        }
        drop(input);
        child.wait()
    });
    let status = match status {
        Ok(s) => s,
        Err(e) => {
            let _ = child.kill();
            return Err(e);
        }
    };
    let text = errors.join().unwrap_or_default();
    if cancel.load(Ordering::SeqCst) {
        return Err(cancelled());
    }
    if !status.success() {
        return Err(client_error(&text, "the import failed"));
    }
    progress(total, Some(total));
    Ok(())
}

/// Writes database `db` to `dest` (through a `.part` file, so a failed or
/// cancelled export leaves nothing half-written). Returns the size written.
pub fn export(
    layout: &Layout,
    version: &str,
    port: u16,
    db: &str,
    dest: &Path,
    progress: &mut dyn FnMut(u64, Option<u64>),
    cancel: &AtomicBool,
) -> io::Result<u64> {
    check_name(db)?;
    let part = PathBuf::from(format!("{}.part", dest.display()));
    let mut child = client(layout, version, "mariadb-dump.exe", port)
        .args(["--single-transaction", "--routines", "--triggers", "--hex-blob", db])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let errors = stderr_text(&mut child);
    let (result, status) = with_cancel(child.id(), cancel, || {
        let result = (|| {
            let mut output = child
                .stdout
                .take()
                .ok_or_else(|| io::Error::other("no output from mariadb-dump"))?;
            let mut file = File::create(&part)?;
            let mut buf = vec![0u8; CHUNK];
            let mut written = 0u64;
            loop {
                let n = output.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                file.write_all(&buf[..n])?;
                written += n as u64;
                progress(written, None);
            }
            file.sync_all()?;
            Ok::<u64, io::Error>(written)
        })();
        if result.is_err() {
            let _ = child.kill();
        }
        (result, child.wait())
    });
    let status = status?;
    let text = errors.join().unwrap_or_default();

    let outcome = if cancel.load(Ordering::SeqCst) {
        Err(cancelled())
    } else if !status.success() {
        Err(client_error(&text, "the export failed"))
    } else {
        result
    };
    match outcome {
        Ok(size) => {
            let _ = std::fs::remove_file(dest);
            std::fs::rename(&part, dest)?;
            Ok(size)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    /// Needs PHPev's MariaDB running (`cargo test -- --ignored`).
    #[test]
    #[ignore]
    fn import_export_and_cancel_against_the_running_server() {
        let layout = Layout::resolve().unwrap();
        let port = Config::load(&layout.config_file()).unwrap().ports.mariadb;
        let version = mariadb::installed(&layout)
            .into_iter()
            .next()
            .expect("MariaDB installed");
        let dir = std::env::temp_dir().join(format!("phpev-dump-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = "phpev_dump_test";
        let never = AtomicBool::new(false);

        let mut sql = String::from(
            "CREATE TABLE t (id INT PRIMARY KEY, name VARCHAR(40));
",
        );
        for i in 0..2000 {
            sql.push_str(&format!(
                "INSERT INTO t VALUES ({i}, 'row {i} — ünïcode');
"
            ));
        }
        let file = dir.join("in.sql");
        std::fs::write(&file, &sql).unwrap();
        let mut last = (0, None);
        import(
            &layout,
            &version,
            port,
            db,
            &file,
            true,
            &mut |d, t| last = (d, t),
            &never,
        )
        .unwrap();
        assert_eq!(last, (sql.len() as u64, Some(sql.len() as u64)));

        let out = dir.join("out.sql");
        let size = export(&layout, &version, port, db, &out, &mut |_, _| {}, &never).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        assert_eq!(size, text.len() as u64);
        assert!(text.contains("CREATE TABLE `t`") && text.contains("row 1999 — ünïcode"));
        assert!(!dir.join("out.sql.part").exists());

        // A bad statement fails with the server's message.
        std::fs::write(
            &file,
            "SELECT * FROM missing_table;
",
        )
        .unwrap();
        let err = import(&layout, &version, port, db, &file, false, &mut |_, _| {}, &never).unwrap_err();
        assert!(err.to_string().contains("missing_table"), "{err}");

        // Cancelling stops a long import early.
        let big: String = (0..400_000)
            .map(|i| {
                format!(
                    "INSERT INTO t VALUES ({}, 'x');
",
                    10_000 + i
                )
            })
            .collect();
        std::fs::write(&file, big).unwrap();
        let cancel = AtomicBool::new(false);
        let started = std::time::Instant::now();
        let err = std::thread::scope(|s| {
            s.spawn(|| {
                std::thread::sleep(Duration::from_millis(400));
                cancel.store(true, Ordering::SeqCst);
            });
            import(&layout, &version, port, db, &file, false, &mut |_, _| {}, &cancel).unwrap_err()
        });
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "took {:?}",
            started.elapsed()
        );

        mariadb::exec_sql(&layout, &version, port, &format!("DROP DATABASE `{db}`")).unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }
}
