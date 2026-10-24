//! php-cgi worker pools, one per PHP version (decided 2026-10-23).
//!
//! php-cgi on Windows serves one request at a time and never forks, so the
//! router leases a whole worker per request. A pool starts with `POOL_MIN`
//! workers, grows to `max_workers()` while every worker is busy, shrinks back
//! after `SHRINK_AFTER` idle, and stops entirely after `POOL_IDLE_STOP` unused.

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use phpev_core::layout::Layout;
use phpev_core::{ipc, php, procinfo};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, Notify};

use crate::log::log_info;
use crate::services::CREATE_NO_WINDOW;
use crate::supervise::{free_port, stop_all, wait_listening, Supervised};

pub const POOL_MIN: usize = 2;
const POOL_MAX_CAP: usize = 8;
/// A pool unused this long is stopped entirely.
pub const POOL_IDLE_STOP: Duration = Duration::from_secs(10 * 60);
/// Extra workers idle this long are stopped (down to `POOL_MIN`).
const SHRINK_AFTER: Duration = Duration::from_secs(60);
/// How long a request waits for a free worker before 503.
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(30);
const PHP_READY_TIMEOUT: Duration = Duration::from_secs(10);

/// min(logical CPUs, 8)
pub fn max_workers() -> usize {
    std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .clamp(POOL_MIN, POOL_MAX_CAP)
}

struct Worker {
    id: u64,
    sup: Supervised,
    busy: bool,
    /// Kept-alive FastCGI connection, returned by the last lease.
    conn: Option<TcpStream>,
    idle_since: Instant,
}

struct Pool {
    workers: Vec<Worker>,
    /// Workers being started right now (count toward the maximum).
    spawning: usize,
    last_used: Instant,
    freed: Arc<Notify>,
}

/// Exclusive use of one worker for one request.
pub struct Lease {
    pub version: String,
    id: u64,
    pub port: u16,
    /// A reusable connection to the worker, if the previous request left one.
    pub conn: Option<TcpStream>,
}

#[derive(Default)]
pub struct Pools {
    pools: Mutex<HashMap<String, Pool>>,
    next_id: AtomicU64,
}

impl Pools {
    /// Leases an idle worker of `version` (an installed directory name),
    /// starting the pool or another worker if needed, else waiting.
    pub async fn acquire(&self, layout: &Layout, version: &str) -> io::Result<Lease> {
        let deadline = Instant::now() + ACQUIRE_TIMEOUT;
        let max = max_workers();
        self.ensure_min(layout, version).await?;
        loop {
            let freed = {
                let mut pools = self.pools.lock().await;
                let pool = pools
                    .get_mut(version)
                    .ok_or_else(|| io::Error::other("php-cgi pool was stopped"))?;
                pool.last_used = Instant::now();
                if let Some(w) = pool
                    .workers
                    .iter_mut()
                    .find(|w| !w.busy && w.sup.shared.pid().is_some())
                {
                    w.busy = true;
                    return Ok(Lease {
                        version: version.to_owned(),
                        id: w.id,
                        port: w.sup.shared.port(),
                        conn: w.conn.take(),
                    });
                }
                if pool.workers.len() + pool.spawning < max {
                    pool.spawning += 1;
                    None
                } else {
                    Some(pool.freed.clone())
                }
            };

            match freed {
                // Every worker is busy and the pool is at its maximum: wait for a release.
                Some(freed) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if tokio::time::timeout(left, freed.notified()).await.is_err() {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            format!("all {max} PHP {version} workers stayed busy for {ACQUIRE_TIMEOUT:?}"),
                        ));
                    }
                }
                // Grow the pool; the new worker goes straight to this request.
                None => {
                    let started = self.start_worker(layout, version).await;
                    let mut pools = self.pools.lock().await;
                    let Some(pool) = pools.get_mut(version) else { continue };
                    pool.spawning -= 1;
                    let mut w = started?;
                    w.busy = true;
                    let lease = Lease {
                        version: version.to_owned(),
                        id: w.id,
                        port: w.sup.shared.port(),
                        conn: None,
                    };
                    log_info!("php-cgi pool {version} grew to {} workers", pool.workers.len() + 1);
                    pool.workers.push(w);
                    return Ok(lease);
                }
            }
        }
    }

    /// Returns a worker. Pass the connection back only if the response was read
    /// to the end (the worker is then ready for the next request on it).
    pub async fn release(&self, lease: Lease, conn: Option<TcpStream>) {
        let mut pools = self.pools.lock().await;
        let Some(pool) = pools.get_mut(&lease.version) else {
            return;
        };
        if let Some(w) = pool.workers.iter_mut().find(|w| w.id == lease.id) {
            w.busy = false;
            w.conn = conn;
            w.idle_since = Instant::now();
        }
        pool.freed.notify_one();
    }

    /// Starts the pool with `POOL_MIN` workers if it is not running.
    pub async fn ensure_min(&self, layout: &Layout, version: &str) -> io::Result<usize> {
        loop {
            {
                let mut pools = self.pools.lock().await;
                let pool = pools.entry(version.to_owned()).or_insert_with(|| Pool {
                    workers: Vec::new(),
                    spawning: 0,
                    last_used: Instant::now(),
                    freed: Arc::new(Notify::new()),
                });
                pool.last_used = Instant::now();
                if pool.workers.len() + pool.spawning >= POOL_MIN {
                    return Ok(pool.workers.len());
                }
                pool.spawning += 1;
            }
            let started = self.start_worker(layout, version).await;
            let mut pools = self.pools.lock().await;
            if let Some(pool) = pools.get_mut(version) {
                pool.spawning -= 1;
                pool.workers.push(started?);
                pool.freed.notify_one();
            }
        }
    }

    async fn start_worker(&self, layout: &Layout, version: &str) -> io::Result<Worker> {
        let dir = php::version_dir(layout, version);
        // php.ini must be reachable through an ASCII path (spike S3).
        let ini_dir = php::php_path(&dir)?;
        let (child, port) = spawn_php_cgi(&dir, &ini_dir).await?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let respawn = move || {
            let (d, i) = (dir.clone(), ini_dir.clone());
            async move { spawn_php_cgi(&d, &i).await }
        };
        let name = format!("php-cgi {version} #{id}");
        log_info!("{name} listening on 127.0.0.1:{port}");
        // php-cgi has no graceful-stop protocol: kill it right away.
        let sup = Supervised::new(name, child, port, Duration::ZERO, respawn, |_| async {});
        Ok(Worker {
            id,
            sup,
            busy: false,
            conn: None,
            idle_since: Instant::now(),
        })
    }

    /// Periodic housekeeping: stop unused pools, shrink idle extra workers.
    pub async fn reap_idle(&self) {
        let mut to_stop = Vec::new();
        {
            let mut pools = self.pools.lock().await;
            pools.retain(|version, pool| {
                if pool.last_used.elapsed() > POOL_IDLE_STOP && pool.workers.iter().all(|w| !w.busy) {
                    log_info!("stopping idle php-cgi pool {version}");
                    to_stop.extend(pool.workers.drain(..).map(|w| w.sup));
                    return false;
                }
                while pool.workers.len() > POOL_MIN {
                    let Some(i) = pool
                        .workers
                        .iter()
                        .position(|w| !w.busy && w.idle_since.elapsed() > SHRINK_AFTER)
                    else {
                        break;
                    };
                    to_stop.push(pool.workers.remove(i).sup);
                    log_info!("php-cgi pool {version} shrank to {} workers", pool.workers.len());
                }
                true
            });
        }
        stop_all(to_stop).await;
    }

    /// Stops one version's pool (its php.ini changed); returns how many workers.
    pub async fn stop_version(&self, version: &str) -> usize {
        let removed = self.pools.lock().await.remove(version);
        let workers: Vec<Supervised> = removed
            .into_iter()
            .flat_map(|p| p.workers.into_iter().map(|w| w.sup))
            .collect();
        let n = workers.len();
        stop_all(workers).await;
        if n > 0 {
            log_info!("php-cgi pool {version} stopped ({n} workers) for a php.ini change");
        }
        n
    }

    pub async fn stop_all(&self) {
        let workers: Vec<Supervised> = self
            .pools
            .lock()
            .await
            .drain()
            .flat_map(|(_, p)| p.workers.into_iter().map(|w| w.sup))
            .collect();
        stop_all(workers).await;
    }

    pub async fn status(&self) -> Vec<ipc::Pool> {
        let pools = self.pools.lock().await;
        let mut out: Vec<ipc::Pool> = pools
            .iter()
            .map(|(version, pool)| ipc::Pool {
                version: version.clone(),
                workers: pool
                    .workers
                    .iter()
                    .map(|w| {
                        let pid = w.sup.shared.pid();
                        ipc::Worker {
                            pid: pid.unwrap_or(0),
                            port: w.sup.shared.port(),
                            memory_kb: pid.and_then(procinfo::memory_kb),
                        }
                    })
                    .collect(),
            })
            .collect();
        out.sort_by(|a, b| phpev_core::version::cmp(&b.version, &a.version));
        out
    }
}

async fn spawn_php_cgi(dir: &Path, ini_dir: &Path) -> io::Result<(Child, u16)> {
    let port = free_port()?;
    let mut child = Command::new(dir.join("php-cgi.exe"))
        .arg("-b")
        .arg(format!("127.0.0.1:{port}"))
        .env("PHPRC", ini_dir)
        // Never exit after N requests; the supervisor restarts it if it dies anyway.
        .env("PHP_FCGI_MAX_REQUESTS", "0")
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    wait_listening(&mut child, port, PHP_READY_TIMEOUT)
        .await
        .map_err(|e| io::Error::new(e.kind(), format!("php-cgi {}: {e}", dir.display())))?;
    Ok((child, port))
}
