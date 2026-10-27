//! Request metrics.
//!
//! Two structures, both updated under one short lock per request:
//! * per-second aggregates (count, status classes, latency histograms) for the
//!   last 10 minutes — accurate at any request rate, O(1) per request;
//! * a ring buffer of the last `CAPACITY` requests, only for "slowest routes".
//!
//! Statistics are computed when someone looks (the dashboard), never per request.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use phpev_core::api::{RequestStats, RouteStats, StatusCounts};

/// Requests kept for route statistics (lost on daemon restart; decided 2026-10-23).
pub const CAPACITY: usize = 2_000;
/// Seconds of aggregates kept (the longest window the API accepts).
const KEEP_SECONDS: u64 = 600;
/// Slowest routes reported.
const TOP_ROUTES: usize = 10;
/// Displayed histogram: upper bounds in ms (last bucket is open-ended).
const BUCKETS_MS: [f64; 10] = [1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0];
/// Fine log-scale buckets for percentiles: bucket i covers up to
/// `FINE_BASE_MS * FINE_GROWTH^i` ms (0.1 ms … ~22 s, about ±15 %).
const FINE: usize = 48;
const FINE_BASE_MS: f64 = 0.1;
const FINE_GROWTH: f64 = 1.3;

pub struct Record {
    pub at_ms: u64,
    pub host: String,
    pub method: &'static str,
    /// Path without query string (never stored).
    pub path: String,
    pub status: u16,
    pub micros: u32,
    pub php: bool,
}

#[derive(Clone, Copy)]
struct Agg {
    count: u32,
    status: [u32; 4],
    coarse: [u32; BUCKETS_MS.len() + 1],
    fine: [u32; FINE],
}

impl Default for Agg {
    fn default() -> Self {
        Self {
            count: 0,
            status: [0; 4],
            coarse: [0; BUCKETS_MS.len() + 1],
            fine: [0; FINE],
        }
    }
}

impl Agg {
    fn add(&mut self, ms: f64, status: u16) {
        self.count += 1;
        if let Some(slot) = (status / 100).checked_sub(2).filter(|s| *s < 4) {
            self.status[slot as usize] += 1;
        }
        self.coarse[BUCKETS_MS.iter().position(|&ub| ms <= ub).unwrap_or(BUCKETS_MS.len())] += 1;
        self.fine[fine_bucket(ms)] += 1;
    }

    fn merge(&mut self, other: &Agg) {
        self.count += other.count;
        self.status.iter_mut().zip(other.status).for_each(|(a, b)| *a += b);
        self.coarse.iter_mut().zip(other.coarse).for_each(|(a, b)| *a += b);
        self.fine.iter_mut().zip(other.fine).for_each(|(a, b)| *a += b);
    }

    /// Percentile estimated from the fine histogram (geometric middle of the bucket).
    fn percentile(&self, q: f64) -> Option<f64> {
        if self.count == 0 {
            return None;
        }
        let rank = ((q * f64::from(self.count)).ceil() as u32).clamp(1, self.count);
        let mut seen = 0;
        for (i, n) in self.fine.iter().enumerate() {
            seen += n;
            if seen >= rank {
                let upper = fine_bound(i);
                let lower = if i == 0 { 0.0 } else { fine_bound(i - 1) };
                return Some(if i == 0 { upper / 2.0 } else { (lower * upper).sqrt() });
            }
        }
        Some(fine_bound(FINE - 1))
    }
}

fn fine_bound(i: usize) -> f64 {
    FINE_BASE_MS * FINE_GROWTH.powi(i as i32)
}

fn fine_bucket(ms: f64) -> usize {
    if ms <= FINE_BASE_MS {
        return 0;
    }
    ((ms / FINE_BASE_MS).ln() / FINE_GROWTH.ln())
        .ceil()
        .clamp(0.0, (FINE - 1) as f64) as usize
}

/// One second of traffic: everything, and PHP requests only.
#[derive(Clone, Copy)]
struct Second {
    sec: u64,
    all: Agg,
    php: Agg,
}

/// Sparse per-second aggregates, oldest first (seconds without traffic take no space).
#[derive(Default)]
struct Series(VecDeque<Second>);

impl Series {
    fn add(&mut self, sec: u64, ms: f64, status: u16, php: bool) {
        // Requests finish slightly out of order; look back a few seconds at most.
        let slot = match self.0.iter().rposition(|s| s.sec <= sec) {
            Some(i) if self.0[i].sec == sec => i,
            Some(i) => {
                self.0.insert(
                    i + 1,
                    Second {
                        sec,
                        all: Agg::default(),
                        php: Agg::default(),
                    },
                );
                i + 1
            }
            None => {
                self.0.push_front(Second {
                    sec,
                    all: Agg::default(),
                    php: Agg::default(),
                });
                0
            }
        };
        let s = &mut self.0[slot];
        s.all.add(ms, status);
        if php {
            s.php.add(ms, status);
        }
        while self.0.front().is_some_and(|f| f.sec + KEEP_SECONDS < sec) {
            self.0.pop_front();
        }
    }
}

#[derive(Default)]
struct Inner {
    ring: VecDeque<Record>,
    all_hosts: Series,
    by_host: HashMap<String, Series>,
}

#[derive(Default)]
pub struct Metrics {
    inner: Mutex<Inner>,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// `PHPEV_NO_METRICS=1` disables recording (used to measure its overhead).
fn disabled() -> bool {
    static OFF: OnceLock<bool> = OnceLock::new();
    *OFF.get_or_init(|| std::env::var_os("PHPEV_NO_METRICS").is_some_and(|v| v == "1"))
}

pub fn method_name(m: &http::Method) -> &'static str {
    match *m {
        http::Method::GET => "GET",
        http::Method::POST => "POST",
        http::Method::PUT => "PUT",
        http::Method::PATCH => "PATCH",
        http::Method::DELETE => "DELETE",
        http::Method::HEAD => "HEAD",
        http::Method::OPTIONS => "OPTIONS",
        _ => "OTHER",
    }
}

impl Metrics {
    pub fn record(&self, r: Record) {
        if disabled() {
            return;
        }
        let ms = f64::from(r.micros) / 1000.0;
        let sec = r.at_ms / 1000;
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.all_hosts.add(sec, ms, r.status, r.php);
        match inner.by_host.get_mut(&r.host) {
            Some(series) => series.add(sec, ms, r.status, r.php),
            None => {
                let mut series = Series::default();
                series.add(sec, ms, r.status, r.php);
                inner.by_host.insert(r.host.clone(), series);
            }
        }
        if inner.ring.len() == CAPACITY {
            inner.ring.pop_front();
        }
        inner.ring.push_back(r);
    }

    /// PHP requests per host over the last `window` seconds (Home's site cards).
    pub fn host_counts(&self, window: u32) -> HashMap<String, u64> {
        let since = (now_ms() / 1000).saturating_sub(u64::from(window.clamp(1, KEEP_SECONDS as u32)));
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner
            .by_host
            .iter()
            .map(|(host, series)| {
                let count = series
                    .0
                    .iter()
                    .filter(|s| s.sec >= since)
                    .map(|s| u64::from(s.php.count))
                    .sum();
                (host.clone(), count)
            })
            .collect()
    }

    /// Statistics for the last `window` complete seconds (≤ 600), optionally for
    /// one host and with or without static files. The running second is left out:
    /// its partial count would show as a dip at the end of every chart.
    pub fn stats(&self, window: u32, host: Option<&str>, include_static: bool) -> RequestStats {
        let now = now_ms();
        let window = window.clamp(1, KEEP_SECONDS as u32);
        let current_sec = now / 1000;
        let first_sec = current_sec - u64::from(window);
        let since_ms = first_sec * 1000;

        // Copy what is needed so the lock is held only briefly.
        let (seconds, rows, mut hosts) = {
            let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            let series = match host {
                Some(h) => inner.by_host.get(h),
                None => Some(&inner.all_hosts),
            };
            let seconds: Vec<(u64, Agg)> = series
                .into_iter()
                .flat_map(|s| s.0.iter())
                .filter(|s| (first_sec..current_sec).contains(&s.sec))
                .map(|s| (s.sec, if include_static { s.all } else { s.php }))
                .collect();
            let rows: Vec<(String, &'static str, String, u32)> = inner
                .ring
                .iter()
                .filter(|r| {
                    r.at_ms >= since_ms
                        && r.at_ms < current_sec * 1000
                        && (include_static || r.php)
                        && host.is_none_or(|h| r.host == h)
                })
                .map(|r| (r.host.clone(), r.method, r.path.clone(), r.micros))
                .collect();
            let hosts: Vec<String> = inner.by_host.keys().cloned().collect();
            (seconds, rows, hosts)
        };
        hosts.sort();

        let w = window as usize;
        let mut per_second = vec![0u32; w];
        let mut p50_per_second = vec![None; w];
        let mut p95_per_second = vec![None; w];
        let mut total = Agg::default();
        for (sec, agg) in &seconds {
            let slot = ((sec - first_sec) as usize).min(w - 1);
            per_second[slot] += agg.count;
            p50_per_second[slot] = agg.percentile(0.50);
            p95_per_second[slot] = agg.percentile(0.95);
            total.merge(agg);
        }

        let mut routes: HashMap<(String, &'static str, String), Vec<f64>> = HashMap::new();
        for (host, method, path, micros) in rows {
            routes
                .entry((host, method, normalize(&path)))
                .or_default()
                .push(f64::from(micros) / 1000.0);
        }
        let mut slowest: Vec<RouteStats> = routes
            .into_iter()
            .map(|((host, method, route), mut l)| {
                l.sort_by(f64::total_cmp);
                RouteStats {
                    host,
                    method: method.to_owned(),
                    route,
                    count: l.len() as u32,
                    avg_ms: l.iter().sum::<f64>() / l.len() as f64,
                    p95_ms: exact_percentile(&l, 0.95),
                    max_ms: *l.last().unwrap_or(&0.0),
                }
            })
            .collect();
        slowest.sort_by(|a, b| b.p95_ms.total_cmp(&a.p95_ms));
        slowest.truncate(TOP_ROUTES);

        let mut labels: Vec<String> = BUCKETS_MS.iter().map(|b| format!("≤{b} ms")).collect();
        labels.push(format!(">{} ms", BUCKETS_MS[BUCKETS_MS.len() - 1]));
        RequestStats {
            window_secs: window,
            generated_ms: now,
            total: u64::from(total.count),
            per_second,
            p50_per_second,
            p95_per_second,
            p50_ms: total.percentile(0.50),
            p95_ms: total.percentile(0.95),
            p99_ms: total.percentile(0.99),
            histogram: labels.into_iter().zip(total.coarse).collect(),
            status: StatusCounts {
                s2xx: total.status[0],
                s3xx: total.status[1],
                s4xx: total.status[2],
                s5xx: total.status[3],
            },
            slowest,
            hosts,
        }
    }
}

/// Nearest-rank percentile of sorted, non-empty values.
fn exact_percentile(sorted: &[f64], q: f64) -> f64 {
    let rank = ((q * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    sorted[rank - 1]
}

/// Groups URLs into routes: numeric segments → `{n}`, UUIDs → `{uuid}`, long
/// hex strings → `{hash}` (decided 2026-10-23).
pub fn normalize(path: &str) -> String {
    let segs: Vec<String> = path
        .split('/')
        .map(|s| {
            if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
                "{n}".to_owned()
            } else if is_uuid(s) {
                "{uuid}".to_owned()
            } else if s.len() >= 16 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
                "{hash}".to_owned()
            } else {
                s.to_owned()
            }
        })
        .collect();
    let joined = segs.join("/");
    if joined.is_empty() {
        "/".to_owned()
    } else {
        joined
    }
}

fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(at_ms: u64, path: &str, status: u16, ms: u32, php: bool) -> Record {
        Record {
            at_ms,
            host: "a.localhost".into(),
            method: "GET",
            path: path.into(),
            status,
            micros: ms * 1000,
            php,
        }
    }

    fn close(actual: Option<f64>, expected: f64) -> bool {
        actual.is_some_and(|a| (a - expected).abs() <= expected * 0.2)
    }

    #[test]
    fn normalizes_ids() {
        assert_eq!(normalize("/users/42/edit"), "/users/{n}/edit");
        assert_eq!(normalize("/o/3f2a1b2c-1234-4abc-9def-0123456789ab"), "/o/{uuid}");
        assert_eq!(normalize("/files/0123456789abcdef0123"), "/files/{hash}");
        assert_eq!(normalize("/"), "/");
        assert_eq!(normalize("/v2/api"), "/v2/api");
    }

    #[test]
    fn stats_over_window() {
        let m = Metrics::default();
        let now = now_ms();
        m.record(rec(now - 1_500, "/users/1", 200, 10, true));
        m.record(rec(now - 1_400, "/users/2", 200, 30, true));
        m.record(rec(now - 1_300, "/users/3", 500, 50, true));
        m.record(rec(now - 1_200, "/app.css", 200, 1, false));
        m.record(rec(now - 120_000, "/old", 200, 999, true)); // outside a 60 s window

        let s = m.stats(60, None, false);
        assert_eq!(s.total, 3);
        assert_eq!(s.per_second.len(), 60);
        assert_eq!(s.per_second.iter().sum::<u32>(), 3);
        assert_eq!((s.status.s2xx, s.status.s5xx), (2, 1));
        assert!(close(s.p50_ms, 30.0), "{:?}", s.p50_ms);
        assert_eq!(s.slowest.len(), 1);
        assert_eq!((s.slowest[0].route.as_str(), s.slowest[0].count), ("/users/{n}", 3));
        assert_eq!(s.slowest[0].max_ms, 50.0);

        assert_eq!(m.stats(60, None, true).total, 4);
        assert_eq!(m.stats(600, None, false).total, 4);
        assert_eq!(m.stats(60, Some("other.localhost"), true).total, 0);
        assert_eq!(m.stats(60, Some("a.localhost"), true).total, 4);
    }

    /// The fix for the M9 finding: counts stay exact far beyond the ring buffer.
    #[test]
    fn rates_stay_exact_beyond_the_ring_buffer() {
        let m = Metrics::default();
        let now = now_ms();
        for i in 0..(CAPACITY as u64 * 5) {
            m.record(rec(now - (i % 30 + 1) * 1000, "/x", 200, 5, true));
        }
        let s = m.stats(60, None, false);
        assert_eq!(s.total, CAPACITY as u64 * 5);
        assert!(close(s.p95_ms, 5.0), "{:?}", s.p95_ms);
        assert_eq!(m.inner.lock().unwrap().ring.len(), CAPACITY);
    }

    #[test]
    fn percentile_buckets_are_accurate_enough() {
        for ms in [0.05, 0.4, 1.0, 3.7, 12.0, 85.0, 640.0, 4_000.0] {
            let mut a = Agg::default();
            a.add(ms, 200);
            assert!(
                close(a.percentile(0.5), ms) || ms < 0.1,
                "{ms} → {:?}",
                a.percentile(0.5)
            );
        }
    }
}
