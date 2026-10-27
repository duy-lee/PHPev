//! Response helpers shared by the site router and the dashboard.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use http::header;
use http::{Response, StatusCode};
use http_body::Frame;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use tokio::sync::mpsc;

pub type Body = BoxBody<Bytes, io::Error>;

/// A body fed chunk by chunk from a task (streamed PHP output, large files, SSE).
pub struct ChannelBody(pub mpsc::Receiver<Bytes>);

impl http_body::Body for ChannelBody {
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, io::Error>>> {
        self.0.poll_recv(cx).map(|chunk| chunk.map(|b| Ok(Frame::data(b))))
    }
}

pub fn full(data: impl Into<Bytes>) -> Body {
    Full::new(data.into()).map_err(|never| match never {}).boxed()
}

pub fn empty() -> Body {
    full(Bytes::new())
}

pub fn internal() -> Response<Body> {
    let mut r = Response::new(empty());
    *r.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
    r
}

pub fn text(status: StatusCode, msg: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(full(msg.to_owned()))
        .unwrap_or_else(|_| internal())
}

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Value of `key` in the request's query string, percent-decoded.
pub fn query_param(uri: &http::Uri, key: &str) -> Option<String> {
    uri.query()?.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (k == key).then(|| percent_decode(&v.replace('+', " ")))
    })
}

pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let Some(b) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::percent_decode;

    #[test]
    fn decodes_path_segments() {
        assert_eq!(percent_decode("database%3AMAIN"), "database:MAIN");
        assert_eq!(percent_decode("my%20site"), "my site");
        assert_eq!(percent_decode("caf%C3%A9"), "café");
        assert_eq!(percent_decode("100%"), "100%");
    }
}
