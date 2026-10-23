//! Keeps one child process alive: event-driven exit detection (no polling),
//! restart with exponential backoff, graceful stop on request.

use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::process::Child;
use tokio::sync::{mpsc, oneshot};

use crate::log::{log_info, log_warn};

const BACKOFF_START: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);
/// A child that stayed up this long resets the backoff.
const HEALTHY_AFTER: Duration = Duration::from_secs(60);

/// Observable facts about a supervised process.
#[derive(Default)]
pub struct Shared {
    pid: AtomicU32,
    port: AtomicU16,
    state: Mutex<&'static str>,
}

impl Shared {
    pub fn pid(&self) -> Option<u32> {
        Some(self.pid.load(Ordering::Relaxed)).filter(|&p| p != 0)
    }
    pub fn port(&self) -> u16 {
        self.port.load(Ordering::Relaxed)
    }
    pub fn state(&self) -> &'static str {
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn set(&self, child: Option<&Child>, port: u16, state: &'static str) {
        self.pid
            .store(child.and_then(Child::id).unwrap_or(0), Ordering::Relaxed);
        if port != 0 {
            self.port.store(port, Ordering::Relaxed);
        }
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = state;
    }
}

/// Handle to a supervised process; `stop` shuts it down for good.
pub struct Supervised {
    pub shared: Arc<Shared>,
    stop_tx: mpsc::Sender<oneshot::Sender<()>>,
}

impl Supervised {
    /// Takes over an already started (and ready) `child`. `respawn` starts a
    /// replacement and waits until it is ready. On stop, `graceful` asks the
    /// process on the given port to exit and it gets `grace` to do so before it
    /// is killed; `Duration::ZERO` kills it right away (php-cgi has no
    /// graceful-stop protocol).
    pub fn new<S, SF, G, GF>(name: String, child: Child, port: u16, grace: Duration, respawn: S, graceful: G) -> Self
    where
        S: FnMut() -> SF + Send + 'static,
        SF: Future<Output = io::Result<(Child, u16)>> + Send + 'static,
        G: FnOnce(u16) -> GF + Send + 'static,
        GF: Future<Output = ()> + Send + 'static,
    {
        let shared = Arc::new(Shared::default());
        shared.set(Some(&child), port, "running");
        let (stop_tx, stop_rx) = mpsc::channel(1);
        tokio::spawn(run(name, child, shared.clone(), stop_rx, grace, respawn, graceful));
        Self { shared, stop_tx }
    }

    pub async fn stop(self) {
        let (ack_tx, ack_rx) = oneshot::channel();
        if self.stop_tx.send(ack_tx).await.is_ok() {
            let _ = ack_rx.await;
        }
    }
}

async fn run<S, SF, G, GF>(
    name: String,
    mut child: Child,
    shared: Arc<Shared>,
    mut stop_rx: mpsc::Receiver<oneshot::Sender<()>>,
    grace: Duration,
    mut respawn: S,
    graceful: G,
) where
    S: FnMut() -> SF,
    SF: Future<Output = io::Result<(Child, u16)>>,
    G: FnOnce(u16) -> GF,
    GF: Future<Output = ()>,
{
    let mut started = Instant::now();
    let mut backoff = BACKOFF_START;
    loop {
        let stop = tokio::select! {
            status = child.wait() => {
                log_warn!("{name} exited unexpectedly ({})", status.map_or_else(|e| e.to_string(), |s| s.to_string()));
                None
            }
            ack = stop_rx.recv() => Some(ack),
        };

        if let Some(ack) = stop {
            shared.set(Some(&child), 0, "stopping");
            graceful(shared.port()).await;
            if grace.is_zero() {
                let _ = child.kill().await;
            } else if tokio::time::timeout(grace, child.wait()).await.is_err() {
                log_warn!("{name} did not stop within {grace:?}; killing it");
                let _ = child.kill().await;
            }
            shared.set(None, 0, "stopped");
            log_info!("{name} stopped");
            if let Some(ack) = ack {
                let _ = ack.send(());
            }
            return;
        }

        if started.elapsed() > HEALTHY_AFTER {
            backoff = BACKOFF_START;
        }
        loop {
            shared.set(None, 0, "restarting");
            tokio::select! {
                _ = tokio::time::sleep(backoff) => {}
                ack = stop_rx.recv() => {
                    shared.set(None, 0, "stopped");
                    if let Some(ack) = ack {
                        let _ = ack.send(());
                    }
                    return;
                }
            }
            backoff = (backoff * 2).min(BACKOFF_MAX);
            match respawn().await {
                Ok((c, port)) => {
                    child = c;
                    shared.set(Some(&child), port, "running");
                    started = Instant::now();
                    log_info!("{name} restarted (pid {}, port {port})", child.id().unwrap_or(0));
                    break;
                }
                Err(e) => log_warn!("{name} restart failed: {e}; retrying in {backoff:?}"),
            }
        }
    }
}

/// A port that is free right now on 127.0.0.1 (the OS picks it).
pub fn free_port() -> io::Result<u16> {
    Ok(TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?
        .local_addr()?
        .port())
}

/// Waits until something accepts connections on 127.0.0.1:`port`, failing early
/// if `child` exits first.
pub async fn wait_listening(child: &mut Child, port: u16, timeout: Duration) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    loop {
        // Async connect: a refused localhost connect can take ~1 s on Windows (SYN retries).
        if let Ok(Ok(_)) = tokio::time::timeout(Duration::from_millis(200), tokio::net::TcpStream::connect(addr)).await
        {
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            return Err(io::Error::other(format!("exited during start-up ({status})")));
        }
        if Instant::now() >= deadline {
            let _ = child.kill().await;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("not listening on port {port} after {timeout:?}"),
            ));
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Stops all `processes` concurrently, so shutdown time is the slowest one,
/// not the sum (Windows allows ~20 s at log off).
pub async fn stop_all(processes: Vec<Supervised>) {
    let mut set = tokio::task::JoinSet::new();
    for p in processes {
        set.spawn(p.stop());
    }
    while set.join_next().await.is_some() {}
}
