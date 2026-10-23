//! Minimal registry access (read string/DWORD, write REG_EXPAND_SZ).

use std::io;
use std::ptr;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows_sys::Win32::System::Registry::{
    RegGetValueW, RegSetKeyValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_EXPAND_SZ, RRF_NOEXPAND,
    RRF_RT_REG_DWORD, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
};

use crate::wide::{from_wide, to_wide};

/// Root registry hive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hive {
    CurrentUser,
    LocalMachine,
}

impl Hive {
    fn handle(self) -> HKEY {
        match self {
            Hive::CurrentUser => HKEY_CURRENT_USER,
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
        }
    }
}

fn check(rc: WIN32_ERROR) -> io::Result<bool> {
    match rc {
        ERROR_SUCCESS => Ok(true),
        ERROR_FILE_NOT_FOUND => Ok(false),
        _ => Err(io::Error::from_raw_os_error(rc as i32)),
    }
}

/// Reads a `REG_SZ`/`REG_EXPAND_SZ` value verbatim (`%VARS%` are not expanded).
/// Returns `Ok(None)` if the key or value does not exist.
pub fn read_string(hive: Hive, subkey: &str, value: &str) -> io::Result<Option<String>> {
    let hkey = hive.handle();
    let subkey = to_wide(subkey);
    let value = to_wide(value);
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;

    let mut size: u32 = 0;
    let rc = unsafe {
        RegGetValueW(
            hkey,
            subkey.as_ptr(),
            value.as_ptr(),
            flags,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut size,
        )
    };
    if !check(rc)? {
        return Ok(None);
    }

    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    let rc = unsafe {
        RegGetValueW(
            hkey,
            subkey.as_ptr(),
            value.as_ptr(),
            flags,
            ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if !check(rc)? {
        return Ok(None);
    }
    Ok(Some(from_wide(&buf).to_string_lossy().into_owned()))
}

/// Reads a `REG_DWORD` value. Returns `Ok(None)` if the key or value does not exist.
pub fn read_dword(hive: Hive, subkey: &str, value: &str) -> io::Result<Option<u32>> {
    let hkey = hive.handle();
    let subkey = to_wide(subkey);
    let value = to_wide(value);
    let mut data = 0u32;
    let mut size = 4u32;
    let rc = unsafe {
        RegGetValueW(
            hkey,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            ptr::null_mut(),
            (&mut data as *mut u32).cast(),
            &mut size,
        )
    };
    Ok(check(rc)?.then_some(data))
}

/// Writes a `REG_EXPAND_SZ` value, creating the key if needed.
pub fn write_expand_string(hive: Hive, subkey: &str, value: &str, data: &str) -> io::Result<()> {
    let hkey = hive.handle();
    let subkey = to_wide(subkey);
    let value = to_wide(value);
    let data = to_wide(data);
    let rc = unsafe {
        RegSetKeyValueW(
            hkey,
            subkey.as_ptr(),
            value.as_ptr(),
            REG_EXPAND_SZ,
            data.as_ptr().cast(),
            (data.len() * 2) as u32,
        )
    };
    check(rc).map(|_| ())
}
