//! Tiny keep-alive HTTP/1.1 load generator used for spike S2 (not shipped).
//!
//! `cargo run --release --example httpbench -- <host:port> <Host header> <path> <connections> <seconds>`

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [addr, host, path, conns, secs] = args.as_slice() else {
        eprintln!("usage: httpbench <host:port> <Host> <path> <connections> <seconds>");
        std::process::exit(2);
    };
    let conns: usize = conns.parse().expect("connections");
    let secs: u64 = secs.parse().expect("seconds");
    let deadline = Instant::now() + Duration::from_secs(secs);

    let handles: Vec<_> = (0..conns)
        .map(|_| {
            let (addr, host, path) = (addr.clone(), host.clone(), path.clone());
            std::thread::spawn(move || worker(&addr, &host, &path, deadline))
        })
        .collect();
    let mut lat: Vec<u128> = Vec::new();
    let mut errors = 0;
    for h in handles {
        let (l, e) = h.join().unwrap();
        lat.extend(l);
        errors += e;
    }
    lat.sort_unstable();
    let pct = |p: f64| {
        lat.get(((lat.len() as f64 * p) as usize).min(lat.len().saturating_sub(1)))
            .copied()
            .unwrap_or(0) as f64
            / 1000.0
    };
    println!(
        "{} req in {secs}s = {:.1} req/s | latency ms p50 {:.1} p90 {:.1} p99 {:.1} max {:.1} | errors {errors}",
        lat.len(),
        lat.len() as f64 / secs as f64,
        pct(0.50),
        pct(0.90),
        pct(0.99),
        pct(1.0)
    );
}

fn worker(addr: &str, host: &str, path: &str, deadline: Instant) -> (Vec<u128>, usize) {
    let req = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: httpbench\r\nAccept: */*\r\n\r\n");
    let mut lat = Vec::new();
    let mut errors = 0;
    let mut conn: Option<BufReader<TcpStream>> = None;
    while Instant::now() < deadline {
        let c = match conn.as_mut() {
            Some(c) => c,
            None => match TcpStream::connect(addr) {
                Ok(s) => {
                    s.set_nodelay(true).ok();
                    conn.insert(BufReader::new(s))
                }
                Err(_) => {
                    errors += 1;
                    continue;
                }
            },
        };
        let start = Instant::now();
        let result = c.get_mut().write_all(req.as_bytes()).and_then(|_| read_response(c));
        if errors == 0 {
            if let Err(e) = &result {
                eprintln!("first error: {e}");
            } else if let Ok(false) = result {
                eprintln!("first error: non-2xx status");
            }
        }
        let ok = matches!(result, Ok(true));
        if ok {
            lat.push(start.elapsed().as_micros());
        } else {
            errors += 1;
            conn = None;
        }
    }
    (lat, errors)
}

/// Reads one response; true if the status is 2xx.
fn read_response(r: &mut BufReader<TcpStream>) -> std::io::Result<bool> {
    let mut line = String::new();
    r.read_line(&mut line)?;
    let ok = line.split_whitespace().nth(1).is_some_and(|c| c.starts_with('2'));
    if !ok && std::env::var_os("HTTPBENCH_DEBUG").is_some() {
        eprintln!("status line: {line:?}");
    }
    let (mut len, mut chunked) = (None, false);
    loop {
        line.clear();
        r.read_line(&mut line)?;
        let l = line.trim_end();
        if l.is_empty() {
            break;
        }
        let lower = l.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            len = v.trim().parse::<usize>().ok();
        } else if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
            chunked = true;
        }
    }
    if chunked {
        loop {
            line.clear();
            r.read_line(&mut line)?;
            let size = usize::from_str_radix(line.trim().split(';').next().unwrap_or("0"), 16).unwrap_or(0);
            let mut buf = vec![0u8; size + 2];
            r.read_exact(&mut buf)?;
            if size == 0 {
                break;
            }
        }
    } else if let Some(n) = len {
        let mut buf = vec![0u8; n];
        r.read_exact(&mut buf)?;
    }
    Ok(ok)
}
