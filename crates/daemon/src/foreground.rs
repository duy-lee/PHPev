//! Bringing a window to the front from the daemon.
//!
//! The daemon runs in the background, so Windows does not let the windows it
//! opens (Explorer, a terminal, a dialog) take the focus from the dashboard the
//! user just clicked in: they would open behind it. Sharing input with the
//! foreground window's thread for a moment lifts that restriction.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows_sys::Win32::System::Threading::AttachThreadInput;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

/// Raises `window` above the others and gives it the keyboard focus.
pub fn force(window: HWND) {
    unsafe {
        if IsIconic(window) != 0 {
            ShowWindow(window, SW_RESTORE);
        }
        let thread = GetWindowThreadProcessId(window, std::ptr::null_mut());
        let fg_thread = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
        let attached = fg_thread != 0 && fg_thread != thread && AttachThreadInput(thread, fg_thread, 1) != 0;
        SetForegroundWindow(window);
        BringWindowToTop(window);
        if attached {
            AttachThreadInput(thread, fg_thread, 0);
        }
    }
}

/// Visible top-level windows that have a title.
pub fn windows() -> HashSet<isize> {
    unsafe extern "system" fn each(window: HWND, list: LPARAM) -> BOOL {
        if IsWindowVisible(window) != 0 && GetWindowTextLengthW(window) > 0 {
            (*(list as *mut HashSet<isize>)).insert(window as isize);
        }
        1
    }
    let mut list = HashSet::new();
    unsafe { EnumWindows(Some(each), &mut list as *mut _ as LPARAM) };
    list
}

fn title(window: HWND) -> String {
    let mut buf = [0u16; 512];
    let len = unsafe { GetWindowTextW(window, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

fn class(window: HWND) -> String {
    let mut buf = [0u16; 128];
    let len = unsafe { GetClassNameW(window, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

/// After starting a program that opens a window: waits (on its own thread,
/// up to 5 s) for a window that was not in `before` and brings it to the
/// front. With `folder`, an Explorer window already showing that folder
/// (Explorer may reuse it instead of opening a new one) counts too.
pub fn raise_new_window(before: HashSet<isize>, folder: Option<String>) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let folder = folder.map(|f| f.to_lowercase());
        let explorer_with_folder = |w: isize| {
            folder.as_ref().is_some_and(|f| {
                class(w as HWND) == "CabinetWClass" && title(w as HWND).to_lowercase().contains(f.as_str())
            })
        };
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(120));
            let now = windows();
            let found = now.iter().copied().find(|w| !before.contains(w)).or_else(|| {
                (Instant::now() + Duration::from_millis(1500) > deadline)
                    .then(|| now.iter().copied().find(|w| explorer_with_folder(*w)))
                    .flatten()
            });
            if let Some(window) = found {
                force(window as HWND);
                return;
            }
        }
    });
}
