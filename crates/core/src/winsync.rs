//! Cross-process named mutex (used to serialize config.toml writers).

use std::io;
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

use crate::wide::to_wide;

/// Held named mutex; released and closed on drop.
pub struct NamedMutexGuard(HANDLE);

// The guard only wraps a kernel handle, which may be used from any thread.
unsafe impl Send for NamedMutexGuard {}

/// Acquires the session-wide mutex `Local\<name>`, waiting up to `timeout`.
/// A mutex abandoned by a crashed owner counts as acquired.
pub fn lock(name: &str, timeout: Duration) -> io::Result<NamedMutexGuard> {
    let wide = to_wide(format!(r"Local\{name}"));
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, wide.as_ptr()) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX - 1);
    match unsafe { WaitForSingleObject(handle, ms) } {
        WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(NamedMutexGuard(handle)),
        WAIT_TIMEOUT => {
            unsafe { CloseHandle(handle) };
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("timed out waiting for lock {name}"),
            ))
        }
        _ => {
            let err = io::Error::last_os_error();
            unsafe { CloseHandle(handle) };
            Err(err)
        }
    }
}

impl Drop for NamedMutexGuard {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_locker_waits_for_first() {
        let name = format!("PHPev.Test.{}", std::process::id());
        let guard = lock(&name, Duration::from_secs(1)).unwrap();
        let n = name.clone();
        // Mutex ownership is per thread; another thread must time out while we hold it.
        let blocked = std::thread::spawn(move || lock(&n, Duration::from_millis(100)).is_err())
            .join()
            .unwrap();
        assert!(blocked);
        drop(guard);
        let n = name.clone();
        assert!(std::thread::spawn(move || lock(&n, Duration::from_millis(500)).is_ok())
            .join()
            .unwrap());
    }
}
