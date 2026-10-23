//! Minimal FastCGI client (responder role) for php-cgi (DESIGN_SPEC §4).
//!
//! One request per connection at a time, request id 1, `FCGI_KEEP_CONN` so the
//! router can reuse the TCP connection to a worker instead of reconnecting.

use std::io;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const VERSION: u8 = 1;
const BEGIN_REQUEST: u8 = 1;
const END_REQUEST: u8 = 3;
const PARAMS: u8 = 4;
const STDIN: u8 = 5;
const STDOUT: u8 = 6;
const STDERR: u8 = 7;
const RESPONDER: u16 = 1;
const FLAG_KEEP_CONN: u8 = 1;
const REQUEST_ID: u16 = 1;
/// Largest record body FastCGI allows.
const MAX_CONTENT: usize = 65_535;

/// Encoded name/value pairs for the PARAMS stream.
#[derive(Default)]
pub struct Params(Vec<u8>);

impl Params {
    pub fn add(&mut self, name: &str, value: impl AsRef<[u8]>) -> &mut Self {
        let value = value.as_ref();
        push_len(&mut self.0, name.len());
        push_len(&mut self.0, value.len());
        self.0.extend_from_slice(name.as_bytes());
        self.0.extend_from_slice(value);
        self
    }
}

/// Lengths < 128 take one byte; longer ones four, with the top bit set.
fn push_len(out: &mut Vec<u8>, len: usize) {
    if len < 128 {
        out.push(len as u8);
    } else {
        out.extend_from_slice(&((len as u32) | 0x8000_0000).to_be_bytes());
    }
}

fn push_record(out: &mut Vec<u8>, kind: u8, content: &[u8]) {
    let pad = (8 - content.len() % 8) % 8;
    out.extend_from_slice(&[VERSION, kind]);
    out.extend_from_slice(&REQUEST_ID.to_be_bytes());
    out.extend_from_slice(&(content.len() as u16).to_be_bytes());
    out.extend_from_slice(&[pad as u8, 0]);
    out.extend_from_slice(content);
    out.extend_from_slice(&[0u8; 8][..pad]);
}

/// Appends `data` as a stream of records (an empty `data` terminates the stream).
fn push_stream(out: &mut Vec<u8>, kind: u8, data: &[u8]) {
    if data.is_empty() {
        push_record(out, kind, &[]);
    }
    for chunk in data.chunks(MAX_CONTENT) {
        push_record(out, kind, chunk);
    }
}

/// Sends BEGIN_REQUEST and the complete PARAMS stream in one write; with
/// `no_body`, also the empty STDIN record, so a GET is a single write.
pub async fn begin<W: AsyncWrite + Unpin>(w: &mut W, params: &Params, no_body: bool) -> io::Result<()> {
    let mut out = Vec::with_capacity(params.0.len() + 64);
    let mut body = [0u8; 8];
    body[..2].copy_from_slice(&RESPONDER.to_be_bytes());
    body[2] = FLAG_KEEP_CONN;
    push_record(&mut out, BEGIN_REQUEST, &body);
    push_stream(&mut out, PARAMS, &params.0);
    push_stream(&mut out, PARAMS, &[]);
    if no_body {
        push_stream(&mut out, STDIN, &[]);
    }
    w.write_all(&out).await
}

/// Sends request body bytes; call with an empty slice to end STDIN.
pub async fn stdin<W: AsyncWrite + Unpin>(w: &mut W, data: &[u8]) -> io::Result<()> {
    let mut out = Vec::with_capacity(data.len() + 16);
    push_stream(&mut out, STDIN, data);
    w.write_all(&out).await
}

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    End { app_status: u32 },
}

/// Reads the next record of the response.
pub async fn read_event<R: AsyncRead + Unpin>(r: &mut R) -> io::Result<Event> {
    loop {
        let mut header = [0u8; 8];
        r.read_exact(&mut header).await?;
        let kind = header[1];
        let len = u16::from_be_bytes([header[4], header[5]]) as usize;
        let pad = header[6] as usize;
        let mut content = vec![0u8; len + pad];
        r.read_exact(&mut content).await?;
        content.truncate(len);
        match kind {
            STDOUT if !content.is_empty() => return Ok(Event::Stdout(content)),
            STDERR if !content.is_empty() => return Ok(Event::Stderr(content)),
            END_REQUEST => {
                let status = content
                    .get(..4)
                    .map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
                return Ok(Event::End { app_status: status });
            }
            // Empty STDOUT/STDERR records close those streams; unknown types are skipped.
            _ => continue,
        }
    }
}

/// The CGI response head: status and headers, parsed from the start of STDOUT.
#[derive(Debug, PartialEq, Eq)]
pub struct Head {
    pub status: u16,
    pub headers: Vec<(String, String)>,
}

/// Splits `buf` at the end of the CGI headers. Returns `None` until the blank
/// line has arrived.
pub fn parse_head(buf: &[u8]) -> Option<(Head, usize)> {
    let (end, sep) = match find(buf, b"\r\n\r\n") {
        Some(i) => (i, 4),
        None => (find(buf, b"\n\n")?, 2),
    };
    let text = String::from_utf8_lossy(&buf[..end]);
    let mut status = 200;
    let mut headers = Vec::new();
    for line in text.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let (name, value) = (name.trim(), value.trim());
        if name.eq_ignore_ascii_case("Status") {
            status = value
                .split_whitespace()
                .next()
                .and_then(|c| c.parse().ok())
                .unwrap_or(200);
        } else {
            headers.push((name.to_owned(), value.to_owned()));
        }
    }
    // A Location header without a Status means a redirect (CGI 1.1 §6.2.3).
    if status == 200 && headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("Location")) {
        status = 302;
    }
    Some((Head { status, headers }, end + sep))
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn param_lengths() {
        let mut p = Params::default();
        p.add("A", "b");
        assert_eq!(p.0, [1, 1, b'A', b'b']);
        let long = "x".repeat(200);
        let mut p = Params::default();
        p.add("K", &long);
        assert_eq!(&p.0[..5], &[1, 0x80, 0, 0, 200]);
    }

    #[test]
    fn records_are_padded_to_8() {
        let mut out = Vec::new();
        push_record(&mut out, STDIN, b"abc");
        assert_eq!(out.len(), 8 + 3 + 5);
        assert_eq!(out[6], 5);
    }

    #[test]
    fn big_streams_are_split() {
        let mut out = Vec::new();
        push_stream(&mut out, STDIN, &vec![0u8; MAX_CONTENT + 1]);
        // One full record (65535 + 1 pad) and one 1-byte record (+7 pad).
        assert_eq!(out.len(), (8 + MAX_CONTENT + 1) + (8 + 1 + 7));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reads_stdout_and_end() {
        let mut wire = Vec::new();
        push_record(&mut wire, STDOUT, b"Status: 404 Not Found\r\nX-A: 1\r\n\r\nbody");
        push_record(&mut wire, STDOUT, b"");
        push_record(&mut wire, END_REQUEST, &[0, 0, 0, 7, 0, 0, 0, 0]);
        let mut r = &wire[..];
        let Event::Stdout(out) = read_event(&mut r).await.unwrap() else {
            panic!()
        };
        let (head, at) = parse_head(&out).unwrap();
        assert_eq!(head.status, 404);
        assert_eq!(head.headers, vec![("X-A".to_owned(), "1".to_owned())]);
        assert_eq!(&out[at..], b"body");
        assert_eq!(read_event(&mut r).await.unwrap(), Event::End { app_status: 7 });
    }

    #[test]
    fn location_implies_redirect() {
        let (head, _) = parse_head(b"Location: /login\r\n\r\n").unwrap();
        assert_eq!(head.status, 302);
        assert!(parse_head(b"Content-Type: text/html\r\n").is_none());
    }
}
