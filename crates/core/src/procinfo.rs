//! Per-process facts for `phpev status`.

use std::mem;

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

/// Working set (resident memory) of `pid` in KiB, if the process is accessible.
pub fn memory_kb(pid: u32) -> Option<u64> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut counters: PROCESS_MEMORY_COUNTERS = mem::zeroed();
        counters.cb = mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let ok = K32GetProcessMemoryInfo(handle, &mut counters, counters.cb);
        CloseHandle(handle);
        (ok != 0).then_some(counters.WorkingSetSize as u64 / 1024)
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

#[cfg(test)]
mod tests {
    #[test]
    fn own_exe_path_is_found() {
        assert_eq!(super::exe_path(std::process::id()), std::env::current_exe().ok());
    }

    #[test]
    fn own_memory_is_reported() {
        assert!(super::memory_kb(std::process::id()).is_some_and(|kb| kb > 0));
    }
}
