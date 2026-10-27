//! Opens the dashboard in its Edge app window and gives that window the sharp
//! PHPev icon.
//!
//! Edge builds an `--app` window's icon from the 16 px favicon and stretches it
//! for the taskbar (40 px at 125 %), which looks blurry whatever favicon sizes
//! the page offers. So after opening the window the daemon sets its own icon
//! (from phpev.ico at the real small/large sizes) with WM_SETICON. It is the
//! daemon that does this because icons die with the process that loaded them.
//!
//! Only one dashboard window is kept: opening it again (tray clicks, `phpev
//! dashboard`) brings the existing window to the front.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use phpev_core::launch;
use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FlashWindowEx, GetClassNameW, GetSystemMetrics, GetWindowTextW, IsIconic, IsWindowVisible, LoadImageW,
    SendMessageTimeoutW, SetForegroundWindow, ShowWindow, FLASHWINFO, FLASHW_ALL, FLASHW_TIMERNOFG, HICON, ICON_BIG,
    ICON_SMALL, IMAGE_ICON, LR_DEFAULTCOLOR, SMTO_ABORTIFHUNG, SM_CXICON, SM_CXSMICON, SM_CYICON, SM_CYSMICON,
    SW_RESTORE, WM_GETICON, WM_SETICON,
};

use crate::log::log_warn;
use crate::Daemon;

/// The page title (index.html), which is also the app window's title.
const TITLE: &str = "PHPev";
/// Edge's top-level window class.
const EDGE_CLASS: &str = "Chrome_WidgetWin_1";
/// Edge sets its own icon again when the favicon arrives (and on reloads),
/// so the icon is re-applied while a dashboard window exists.
const POLL: Duration = Duration::from_millis(500);
/// Stop watching once no dashboard window has been seen for this long.
const GIVE_UP: Duration = Duration::from_secs(15);

/// A window just launched takes a moment to appear; clicks within this time
/// do not launch another one.
const STARTING: Duration = Duration::from_secs(5);

static WATCHING: AtomicBool = AtomicBool::new(false);
static LAST_LAUNCH: Mutex<Option<Instant>> = Mutex::new(None);

pub enum Opened {
    Launched,
    /// An existing dashboard window was brought to the front.
    Focused,
    /// One was launched moments ago and is still starting.
    Starting,
}

/// Opens the dashboard (with this run's login URL): an Edge app window, or a
/// tab of the default browser when `browser` is set or Edge is missing.
pub fn open_dashboard(daemon: &Daemon, browser: bool) -> io::Result<Opened> {
    let url = daemon
        .dashboard
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .map_err(io::Error::other)?;
    if browser {
        return launch::open_url(&url).map(|()| Opened::Launched);
    }
    if let Some(&hwnd) = windows().first() {
        focus(hwnd);
        return Ok(Opened::Focused);
    }
    let mut last = LAST_LAUNCH.lock().unwrap_or_else(|e| e.into_inner());
    if last.is_some_and(|t| t.elapsed() < STARTING) {
        return Ok(Opened::Starting);
    }
    let edge = launch::open_app_window(&url, &daemon.layout.dashboard_profile_dir())?;
    *last = Some(Instant::now());
    drop(last);
    if edge && !WATCHING.swap(true, Ordering::AcqRel) {
        let spawned = std::thread::Builder::new()
            .name("appwindow".into())
            .stack_size(64 * 1024)
            .spawn(|| {
                watch();
                WATCHING.store(false, Ordering::Release);
            });
        if let Err(e) = spawned {
            WATCHING.store(false, Ordering::Release);
            log_warn!("dashboard icon thread: {e}");
        }
    }
    Ok(Opened::Launched)
}

/// Restores and activates `hwnd`; if Windows refuses to switch the foreground
/// (the request came from a terminal, not a click on the tray), the taskbar
/// button flashes instead.
fn focus(hwnd: HWND) {
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        if SetForegroundWindow(hwnd) == 0 {
            let flash = FLASHWINFO {
                cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
                hwnd,
                dwFlags: FLASHW_ALL | FLASHW_TIMERNOFG,
                uCount: 0,
                dwTimeout: 0,
            };
            FlashWindowEx(&flash);
        }
    }
}

/// The dashboard app window, if one is open (owner of dialogs the dashboard opens).
pub(crate) fn window() -> Option<HWND> {
    windows().into_iter().next()
}

/// Visible dashboard app windows.
fn windows() -> Vec<HWND> {
    let mut found = Vec::new();
    unsafe { EnumWindows(Some(collect), &mut found as *mut Vec<HWND> as LPARAM) };
    found
}

fn watch() {
    let (small, big) = unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        let load = |w, h| LoadImageW(module, 1 as _, IMAGE_ICON, w, h, LR_DEFAULTCOLOR);
        (
            load(GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON)),
            load(GetSystemMetrics(SM_CXICON), GetSystemMetrics(SM_CYICON)),
        )
    };
    if small.is_null() || big.is_null() {
        return;
    }
    let mut last_seen = Instant::now();
    loop {
        let found = windows();
        if found.is_empty() {
            if last_seen.elapsed() > GIVE_UP {
                return;
            }
        } else {
            last_seen = Instant::now();
            // It has appeared: from now on a missing window means it was closed.
            *LAST_LAUNCH.lock().unwrap_or_else(|e| e.into_inner()) = None;
            for hwnd in found {
                set_icon(hwnd, ICON_BIG, big);
                set_icon(hwnd, ICON_SMALL, small);
            }
        }
        std::thread::sleep(POLL);
    }
}

/// Sets the icon unless the window already has it (a hung window is skipped).
fn set_icon(hwnd: HWND, kind: u32, icon: HICON) {
    let send = |msg: u32, lparam: LPARAM| {
        let mut result = 0usize;
        unsafe { SendMessageTimeoutW(hwnd, msg, kind as WPARAM, lparam, SMTO_ABORTIFHUNG, 200, &mut result) };
        result
    };
    if send(WM_GETICON, 0) != icon as usize {
        send(WM_SETICON, icon as LPARAM);
    }
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    if IsWindowVisible(hwnd) != 0 && text(hwnd, GetClassNameW) == EDGE_CLASS && text(hwnd, GetWindowTextW) == TITLE {
        (*(lparam as *mut Vec<HWND>)).push(hwnd);
    }
    1
}

unsafe fn text(hwnd: HWND, get: unsafe extern "system" fn(HWND, *mut u16, i32) -> i32) -> String {
    let mut buf = [0u16; 64];
    let n = get(hwnd, buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&buf[..n])
}
