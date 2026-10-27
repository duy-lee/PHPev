//! Running a program for a dashboard job: its output line by line, and
//! stopping it when the job is cancelled.

use std::io::{self, BufRead, BufReader, Read};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Ends process `pid` and the processes it started (Composer runs git and
/// unzip; they must not keep running after a cancel).
fn kill_tree(pid: u32) {
    use std::os::windows::process::CommandExt;
    let system = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let killed = Command::new(std::path::Path::new(&system).join(r"System32\taskkill.exe"))
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .is_ok_and(|s| s.success());
    if !killed {
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if !handle.is_null() {
                TerminateProcess(handle, 1);
                CloseHandle(handle);
            }
        }
    }
}

/// Runs `work` while another thread ends process `pid` (and its children) as
/// soon as `cancel` is set: a read or write blocked on the process must not delay it.
pub fn with_cancel<T>(pid: u32, cancel: &AtomicBool, work: impl FnOnce() -> T) -> T {
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                if cancel.load(Ordering::SeqCst) {
                    kill_tree(pid);
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

fn forward(stream: impl Read + Send + 'static, lines: mpsc::Sender<String>) {
    std::thread::spawn(move || {
        // Progress bars redraw with \r: each redraw is a line of its own.
        let mut reader = BufReader::new(stream);
        let mut buf = Vec::new();
        while reader.read_until(b'\n', &mut buf).unwrap_or(0) > 0 {
            for part in String::from_utf8_lossy(&buf).split('\r') {
                let part = part.trim_end();
                if !part.trim().is_empty() && lines.send(part.to_owned()).is_err() {
                    return;
                }
            }
            buf.clear();
        }
    });
}

/// Runs `cmd` with no window, passing each line it prints (stdout and
/// stderr) to `line`. Ends it when `cancel` is set (the result is then an
/// `Interrupted` error).
pub fn run_logged(cmd: &mut Command, line: &mut dyn FnMut(&str), cancel: &AtomicBool) -> io::Result<ExitStatus> {
    use std::os::windows::process::CommandExt;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;
    let (tx, rx) = mpsc::channel();
    if let Some(out) = child.stdout.take() {
        forward(out, tx.clone());
    }
    if let Some(err) = child.stderr.take() {
        forward(err, tx);
    }
    let status = with_cancel(child.id(), cancel, || {
        // A process it started may keep the output open after it exits:
        // stop reading once it has exited and nothing more arrives.
        loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(l) => line(&l),
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if cancel.load(Ordering::SeqCst) || child.try_wait()?.is_some() {
                        while let Ok(l) = rx.recv_timeout(Duration::from_millis(150)) {
                            line(&l);
                        }
                        break;
                    }
                }
            }
        }
        child.wait()
    })?;
    if cancel.load(Ordering::SeqCst) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
    }
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_from_both_streams_and_the_exit_code() {
        let never = AtomicBool::new(false);
        let mut got = Vec::new();
        let status = run_logged(
            Command::new("cmd.exe").args(["/c", "echo one& echo two 1>&2& exit 3"]),
            &mut |l| got.push(l.to_owned()),
            &never,
        )
        .unwrap();
        got.sort();
        assert_eq!(got, ["one", "two"]);
        assert_eq!(status.code(), Some(3));
    }

    #[test]
    fn cancelling_stops_the_program() {
        let cancel = AtomicBool::new(false);
        let started = std::time::Instant::now();
        let err = std::thread::scope(|s| {
            s.spawn(|| {
                std::thread::sleep(Duration::from_millis(300));
                cancel.store(true, Ordering::SeqCst);
            });
            run_logged(
                Command::new("cmd.exe").args(["/c", "ping -n 30 127.0.0.1 >nul"]),
                &mut |_| {},
                &cancel,
            )
            .unwrap_err()
        });
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
