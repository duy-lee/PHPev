//! Finding which process owns a listening TCP port, without binding anything
//! ourselves (binding would trigger the Windows Firewall prompt, §10).

use std::collections::HashMap;
use std::io;
use std::mem;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, INVALID_HANDLE_VALUE, NO_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
};
use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

use crate::wide::from_wide;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listener {
    pub addr: IpAddr,
    pub port: u16,
    pub pid: u32,
    /// Executable name (e.g. `httpd.exe`); `System` for PID 4 (HTTP.sys / IIS).
    pub process: String,
}

/// All TCP listeners (IPv4 and IPv6) on `port`.
pub fn listeners_on(port: u16) -> io::Result<Vec<Listener>> {
    let mut found: Vec<(IpAddr, u16, u32)> = Vec::new();

    for row in tcp_table::<MIB_TCPROW_OWNER_PID>(AF_INET as u32)? {
        let p = u16::from_be(row.dwLocalPort as u16);
        if p == port {
            let ip = Ipv4Addr::from(u32::from_be(row.dwLocalAddr));
            found.push((ip.into(), p, row.dwOwningPid));
        }
    }
    for row in tcp_table::<MIB_TCP6ROW_OWNER_PID>(AF_INET6 as u32)? {
        let p = u16::from_be(row.dwLocalPort as u16);
        if p == port {
            found.push((Ipv6Addr::from(row.ucLocalAddr).into(), p, row.dwOwningPid));
        }
    }

    if found.is_empty() {
        return Ok(Vec::new());
    }
    let names = process_names()?;
    Ok(found
        .into_iter()
        .map(|(addr, port, pid)| Listener {
            addr,
            port,
            pid,
            process: names.get(&pid).cloned().unwrap_or_else(|| format!("pid {pid}")),
        })
        .collect())
}

/// Reads the owner-PID listener table for one address family.
fn tcp_table<Row: Copy>(family: u32) -> io::Result<Vec<Row>> {
    let mut size: u32 = 0;
    let mut buf: Vec<u64> = Vec::new(); // u64 for alignment
    loop {
        let rc = unsafe {
            GetExtendedTcpTable(
                buf.as_mut_ptr().cast(),
                &mut size,
                0,
                family,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        match rc {
            NO_ERROR => break,
            ERROR_INSUFFICIENT_BUFFER => buf = vec![0u64; (size as usize).div_ceil(8)],
            _ => return Err(io::Error::from_raw_os_error(rc as i32)),
        }
    }
    if buf.is_empty() {
        return Ok(Vec::new());
    }

    // Both MIB_TCPTABLE_OWNER_PID and MIB_TCP6TABLE_OWNER_PID are
    // `{ dwNumEntries: u32, table: [Row; N] }`, with the rows aligned after the count.
    let base = buf.as_ptr().cast::<u8>();
    let count = unsafe { *(base as *const u32) } as usize;
    let offset = mem::size_of::<u32>().next_multiple_of(mem::align_of::<Row>());
    let rows = unsafe { std::slice::from_raw_parts(base.add(offset).cast::<Row>(), count) };
    Ok(rows.to_vec())
}

/// PID → executable name for every running process.
fn process_names() -> io::Result<HashMap<u32, String>> {
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let mut names = HashMap::new();
    let mut entry: PROCESSENTRY32W = unsafe { mem::zeroed() };
    entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut ok = unsafe { Process32FirstW(snap, &mut entry) };
    while ok != 0 {
        names.insert(
            entry.th32ProcessID,
            from_wide(&entry.szExeFile).to_string_lossy().into_owned(),
        );
        ok = unsafe { Process32NextW(snap, &mut entry) };
    }
    unsafe { CloseHandle(snap) };
    Ok(names)
}

/// A human hint for well-known port squatters.
pub fn known_culprit(process: &str) -> Option<&'static str> {
    match process.to_ascii_lowercase().as_str() {
        "system" => Some("HTTP.sys (IIS, Web Deploy, or another http.sys user)"),
        "httpd.exe" => Some("Apache (XAMPP / Laragon / WAMP)"),
        "nginx.exe" => Some("nginx (Laragon / Herd / manual install)"),
        "mysqld.exe" | "mariadbd.exe" => Some("another MySQL/MariaDB server (XAMPP / Laragon / service)"),
        "skype.exe" => Some("Skype"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_our_own_listener() {
        let sock = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = sock.local_addr().unwrap().port();
        let found = listeners_on(port).unwrap();
        assert!(found.iter().any(|l| l.pid == std::process::id()), "{found:?}");
    }
}
