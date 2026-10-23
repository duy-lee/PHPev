//! Single-line download progress on stderr.

use std::io::Write;
use std::time::Instant;

pub struct Bar {
    label: String,
    started: Instant,
    last_shown: Option<u64>,
    done: u64,
}

impl Bar {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            started: Instant::now(),
            last_shown: None,
            done: 0,
        }
    }

    pub fn update(&mut self, done: u64, total: Option<u64>) {
        self.done = done;
        // Redraw on each new percent (or each MiB when the size is unknown).
        let step = match total {
            Some(t) if t > 0 => done * 100 / t,
            _ => done >> 20,
        };
        if self.last_shown == Some(step) {
            return;
        }
        self.last_shown = Some(step);
        let line = match total {
            Some(t) if t > 0 => format!("{:>3}%  {} / {}", step, mib(done), mib(t)),
            _ => mib(done),
        };
        eprint!("\r  ↓ {}  {line}   ", self.label);
        let _ = std::io::stderr().flush();
    }

    pub fn finish(&self) {
        if self.last_shown.is_none() {
            return; // nothing was downloaded (e.g. cached)
        }
        let secs = self.started.elapsed().as_secs_f64().max(0.001);
        eprintln!(
            "\r  ↓ {}  {} in {:.1}s ({}/s)          ",
            self.label,
            mib(self.done),
            secs,
            mib((self.done as f64 / secs) as u64)
        );
    }
}

pub fn mib(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}
