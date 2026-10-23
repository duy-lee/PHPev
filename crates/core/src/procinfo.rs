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

#[cfg(test)]
mod tests {
    #[test]
    fn own_memory_is_reported() {
        assert!(super::memory_kb(std::process::id()).is_some_and(|kb| kb > 0));
    }
}
