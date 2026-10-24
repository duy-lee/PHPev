//! Per-process facts for `phpev status` and the dashboard.

use std::mem;

use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX2};
use windows_sys::Win32::System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

/// Private working set of `pid` in KiB — the "Memory" column of Task Manager
/// and the RAM metric PHPev reports (decided 2026-10-23). Shared DLL pages are
/// not counted.
pub fn memory_kb(pid: u32) -> Option<u64> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut counters: PROCESS_MEMORY_COUNTERS_EX2 = mem::zeroed();
        counters.cb = mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32;
        let ok = K32GetProcessMemoryInfo(
            handle,
            (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX2).cast(),
            counters.cb,
        );
        CloseHandle(handle);
        (ok != 0).then_some(counters.PrivateWorkingSetSize as u64 / 1024)
    }
}

/// Total CPU time (kernel + user) used by `pid` so far, in 100 ns units.
pub fn cpu_time(pid: u32) -> Option<u64> {
    let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let (mut created, mut exited, mut kernel, mut user): (FILETIME, FILETIME, FILETIME, FILETIME) =
            (mem::zeroed(), mem::zeroed(), mem::zeroed(), mem::zeroed());
        let ok = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user);
        CloseHandle(handle);
        (ok != 0).then(|| ticks(kernel) + ticks(user))
    }
}

/// Full path of the executable running as `pid`, if accessible.
pub fn exe_path(pid: u32) -> Option<std::path::PathBuf> {
    use windows_sys::Win32::System::Threading::{QueryFullProcessImageNameW, PROCESS_NAME_WIN32};
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(handle);
        (ok != 0).then(|| std::path::PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
    }
}

/// Processes whose executable lives under `dir`, as "name (pid)".
pub fn running_from(dir: &std::path::Path) -> Vec<String> {
    // Either path may use 8.3 names (an executable keeps the form it was
    // started with) or forward slashes, so both are compared canonicalized.
    let lower = |p: std::path::PathBuf| std::path::PathBuf::from(p.to_string_lossy().to_lowercase());
    let Ok(dir) = std::fs::canonicalize(dir).map(lower) else {
        return Vec::new();
    };
    let mut found: Vec<String> = crate::ports::process_names()
        .unwrap_or_default()
        .into_iter()
        .filter(|(pid, _)| {
            exe_path(*pid)
                .and_then(|exe| std::fs::canonicalize(exe).ok())
                .is_some_and(|exe| lower(exe).starts_with(&dir))
        })
        .map(|(pid, name)| format!("{name} ({pid})"))
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    #[test]
    fn own_exe_path_is_found() {
        assert_eq!(super::exe_path(std::process::id()), std::env::current_exe().ok());
    }

    #[test]
    fn finds_processes_running_from_a_folder() {
        let exe = std::env::current_exe().unwrap();
        let own = format!("({})", std::process::id());
        assert!(super::running_from(exe.parent().unwrap())
            .iter()
            .any(|p| p.ends_with(&own)));
        assert!(super::running_from(&exe.parent().unwrap().join("elsewhere")).is_empty());
    }

    #[test]
    fn own_cpu_time_grows() {
        let before = super::cpu_time(std::process::id()).unwrap();
        let mut x = 0u64;
        for i in 0..20_000_000u64 {
            x = x.wrapping_add(i * i);
        }
        assert!(x > 0);
        assert!(super::cpu_time(std::process::id()).unwrap() > before);
    }

    #[test]
    fn own_memory_is_reported() {
        assert!(super::memory_kb(std::process::id()).is_some_and(|kb| kb > 0));
    }
}
