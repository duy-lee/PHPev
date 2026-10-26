//! Long operations started from the dashboard (installs), run on a blocking
//! thread and followed over SSE through a `watch` channel.

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use phpev_core::api::{JobInfo, JobStatus};
use phpev_core::ops::Report;
use tokio::sync::watch;

use crate::log::{log_info, log_warn};

/// Finished jobs kept for late viewers.
const KEEP_FINISHED: usize = 20;
const MAX_LINES: usize = 200;
/// Download progress is published at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(150);

pub struct Job {
    info: Mutex<JobInfo>,
    changed: watch::Sender<u64>,
    /// Set by `cancel`; cancellable work checks it and stops.
    cancel: Arc<AtomicBool>,
}

impl Job {
    pub fn snapshot(&self) -> JobInfo {
        self.info.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }

    fn update(&self, f: impl FnOnce(&mut JobInfo)) {
        f(&mut self.info.lock().unwrap_or_else(|e| e.into_inner()));
        self.changed.send_modify(|v| *v += 1);
    }
}

#[derive(Default)]
pub struct Jobs {
    next: AtomicU64,
    jobs: Mutex<HashMap<u64, Arc<Job>>>,
}

impl Jobs {
    /// Runs `work` on a blocking thread; its `Ok` message becomes the last line.
    pub fn start<F>(&self, title: String, work: F) -> u64
    where
        F: FnOnce(&mut dyn Report) -> io::Result<String> + Send + 'static,
    {
        self.run(title, false, move |report, _| work(report))
    }

    /// Like `start`, for work that stops (with any error) once the flag it
    /// gets is set by `cancel`.
    pub fn start_cancellable<F>(&self, title: String, work: F) -> u64
    where
        F: FnOnce(&mut dyn Report, &AtomicBool) -> io::Result<String> + Send + 'static,
    {
        self.run(title, true, work)
    }

    /// Asks a running cancellable job to stop. False if there is none.
    pub fn cancel(&self, id: u64) -> bool {
        match self.get(id) {
            Some(job) if job.snapshot().cancellable && job.snapshot().status == JobStatus::Running => {
                job.cancel.store(true, Ordering::SeqCst);
                true
            }
            _ => false,
        }
    }

    fn run<F>(&self, title: String, cancellable: bool, work: F) -> u64
    where
        F: FnOnce(&mut dyn Report, &AtomicBool) -> io::Result<String> + Send + 'static,
    {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let (changed, _) = watch::channel(0);
        let job = Arc::new(Job {
            info: Mutex::new(JobInfo {
                id,
                title: title.clone(),
                status: JobStatus::Running,
                lines: Vec::new(),
                progress: None,
                error: None,
                cancellable,
            }),
            changed,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        {
            let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
            jobs.insert(id, job.clone());
            prune(&mut jobs);
        }
        log_info!("job {id} started: {title}");
        tokio::task::spawn_blocking(move || {
            let mut report = JobReport {
                job: job.clone(),
                last_progress: None,
            };
            let cancel = job.cancel.clone();
            let result = work(&mut report, &cancel);
            job.update(|i| {
                i.progress = None;
                match result {
                    Ok(msg) => {
                        i.lines.push(msg);
                        i.status = JobStatus::Ok;
                    }
                    Err(_) if cancel.load(Ordering::SeqCst) => {
                        log_info!("job {id} cancelled");
                        i.status = JobStatus::Cancelled;
                    }
                    Err(e) => {
                        log_warn!("job {id} failed: {e}");
                        i.error = Some(e.to_string());
                        i.status = JobStatus::Error;
                    }
                }
            });
        });
        id
    }

    pub fn get(&self, id: u64) -> Option<Arc<Job>> {
        self.jobs.lock().unwrap_or_else(|e| e.into_inner()).get(&id).cloned()
    }
}

fn prune(jobs: &mut HashMap<u64, Arc<Job>>) {
    let mut finished: Vec<u64> = jobs
        .iter()
        .filter(|(_, j)| j.snapshot().status != JobStatus::Running)
        .map(|(id, _)| *id)
        .collect();
    finished.sort_unstable();
    while finished.len() > KEEP_FINISHED {
        jobs.remove(&finished.remove(0));
    }
}

struct JobReport {
    job: Arc<Job>,
    last_progress: Option<Instant>,
}

impl Report for JobReport {
    fn info(&mut self, msg: &str) {
        self.job.update(|i| {
            i.progress = None;
            i.lines.push(msg.to_owned());
            if i.lines.len() > MAX_LINES {
                i.lines.remove(0);
            }
        });
    }

    fn progress(&mut self, label: &str, done: u64, total: Option<u64>) {
        let finished = total == Some(done);
        if !finished && self.last_progress.is_some_and(|t| t.elapsed() < PROGRESS_EVERY) {
            return;
        }
        self.last_progress = Some(Instant::now());
        self.job.update(|i| i.progress = Some((label.to_owned(), done, total)));
    }
}
