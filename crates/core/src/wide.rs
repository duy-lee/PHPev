//! UTF-16 helpers for Win32 calls.

use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};

/// Encodes a string as a NUL-terminated UTF-16 buffer.
pub fn to_wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

/// Decodes a UTF-16 buffer, stopping at the first NUL if there is one.
pub fn from_wide(buf: &[u16]) -> OsString {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    OsString::from_wide(&buf[..end])
}
