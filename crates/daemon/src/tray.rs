//! Notification-area icon and menu. It lives on the hidden
//! window the daemon already has for end-session messages (endsession.rs), so
//! it costs no extra process or thread.
//!
//! Left click opens the dashboard; right click shows the menu, built fresh each
//! time from config.toml and the daemon state. Slow actions (MariaDB) run on
//! the tokio runtime and report failures as a balloon.

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};

use phpev_core::config::Config;
use phpev_core::{autostart, launch, sites};
use tokio::runtime::Handle;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_WARNING, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetSystemMetrics, LoadIconW, LoadImageW, PostMessageW,
    RegisterWindowMessageW, SetForegroundWindow, SetMenuDefaultItem, TrackPopupMenu, HICON, HMENU, IDI_APPLICATION,
    IMAGE_ICON, LR_DEFAULTCOLOR, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, SM_CXSMICON, SM_CYSMICON,
    TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_CONTEXTMENU, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP,
};

use crate::log::log_warn;
use crate::Daemon;

/// Callback message of the icon (lparam carries the mouse message).
const WM_TRAY: u32 = WM_APP + 1;
/// Posted by `set_visible` from other threads: wparam 1 = show, 0 = hide.
const WM_SET_TRAY: u32 = WM_APP + 2;
const ICON_ID: u32 = 1;

const CMD_DASHBOARD: usize = 1;
const CMD_DB_TOGGLE: usize = 2;
const CMD_AUTOSTART: usize = 3;
const CMD_EXIT: usize = 4;
const CMD_SITE: usize = 100;
/// Sites listed in the menu at most (the dashboard lists all).
const MAX_SITES: usize = 30;

struct Ctx {
    daemon: Arc<Daemon>,
    rt: Handle,
}

static CTX: OnceLock<Ctx> = OnceLock::new();
static WINDOW: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
/// Explorer broadcasts this after it restarts; the icon must then be added again.
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

/// Called once before the window thread starts.
pub fn init(daemon: Arc<Daemon>, rt: Handle) {
    let _ = CTX.set(Ctx { daemon, rt });
}

/// Called on the window thread right after the hidden window exists.
pub fn on_window_created(hwnd: HWND) {
    WINDOW.store(hwnd, Ordering::Release);
    let name = wide("TaskbarCreated");
    TASKBAR_CREATED.store(unsafe { RegisterWindowMessageW(name.as_ptr()) }, Ordering::Relaxed);
    if enabled() {
        add(hwnd);
    }
}

/// Shows or hides the icon now (`phpev tray on|off`); safe from any thread.
pub fn set_visible(on: bool) {
    let hwnd = WINDOW.load(Ordering::Acquire);
    if !hwnd.is_null() {
        unsafe { PostMessageW(hwnd, WM_SET_TRAY, on as usize, 0) };
    }
}

/// Removes the icon before the daemon exits (otherwise a dead icon stays until
/// the mouse passes over it). Safe from any thread.
pub fn remove() {
    let hwnd = WINDOW.load(Ordering::Acquire);
    if !hwnd.is_null() {
        let nid = base(hwnd);
        unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
    }
}

/// Handles the tray's window messages; `None` for anything else.
pub fn handle(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    match msg {
        WM_TRAY => {
            match lparam as u32 {
                WM_LBUTTONUP => open_dashboard(),
                WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
                _ => {}
            }
            Some(0)
        }
        WM_SET_TRAY => {
            if wparam != 0 {
                add(hwnd);
            } else {
                remove();
            }
            Some(0)
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            if enabled() {
                add(hwnd);
            }
            Some(0)
        }
        _ => None,
    }
}

fn enabled() -> bool {
    CTX.get().is_some_and(|c| config(c).is_none_or(|c| c.tray))
}

fn config(ctx: &Ctx) -> Option<Config> {
    Config::load(&ctx.daemon.layout.config_file()).ok()
}

fn base(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = ICON_ID;
    nid
}

fn add(hwnd: HWND) {
    let mut nid = base(hwnd);
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = icon();
    copy(&mut nid.szTip, "PHPev");
    unsafe {
        // Already there (e.g. `phpev tray on` twice): just refresh it.
        if Shell_NotifyIconW(NIM_ADD, &nid) == 0 {
            Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }
}

/// Must run before any window exists. Without it Windows reports 16 px as the
/// small-icon size at every scale and stretches the tray icon (blurry at 125 %+);
/// with it, the icon is loaded at the real size (20 px at 125 %, 24 px at 150 %).
pub fn set_dpi_aware() {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}

/// The embedded app icon (resource 1, see build.rs) at the small-icon size.
fn icon() -> HICON {
    unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        let icon = LoadImageW(
            module,
            1 as _,
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            LR_DEFAULTCOLOR,
        );
        if icon.is_null() {
            LoadIconW(std::ptr::null_mut(), IDI_APPLICATION)
        } else {
            icon
        }
    }
}

/// A balloon next to the icon, for errors of actions started from the menu.
fn notify(title: &str, text: &str) {
    let hwnd = WINDOW.load(Ordering::Acquire);
    if hwnd.is_null() {
        return;
    }
    let mut nid = base(hwnd);
    nid.uFlags = NIF_INFO;
    nid.dwInfoFlags = NIIF_WARNING;
    copy(&mut nid.szInfoTitle, title);
    copy(&mut nid.szInfo, text);
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
}

fn open_dashboard() {
    let Some(ctx) = CTX.get() else { return };
    if let Err(e) = crate::appwindow::open_dashboard(&ctx.daemon, false) {
        log_warn!("tray: could not open the dashboard: {e}");
        notify("PHPev", &format!("Could not open the dashboard: {e}"));
    }
}

fn show_menu(hwnd: HWND) {
    let Some(ctx) = CTX.get() else { return };
    let config = config(ctx).unwrap_or_default();
    let mut site_urls = Vec::new();
    unsafe {
        let menu = CreatePopupMenu();
        append(menu, MF_STRING, CMD_DASHBOARD, "Open dashboard");
        SetMenuDefaultItem(menu, CMD_DASHBOARD as u32, 0);

        let sites_menu = CreatePopupMenu();
        for site in sites::all(&config).iter().take(MAX_SITES) {
            if let Ok(url) = crate::dashboard::site_url(&ctx.daemon.layout, site) {
                append(sites_menu, MF_STRING, CMD_SITE + site_urls.len(), &url);
                site_urls.push(url);
            }
        }
        if site_urls.is_empty() {
            append(sites_menu, MF_STRING | MF_GRAYED, 0, "No sites yet");
        }
        append(menu, MF_POPUP, sites_menu as usize, "Sites");
        append(menu, MF_SEPARATOR, 0, "");

        // try_lock: never block the UI thread while MariaDB is starting/stopping.
        let db_label = match (config.mariadb.as_deref(), ctx.daemon.db.try_lock()) {
            (None, _) => ("MariaDB: not installed".to_owned(), MF_GRAYED),
            (Some(v), Ok(db)) if db.is_some() => (format!("Stop MariaDB {v}"), 0),
            (Some(v), Ok(_)) => (format!("Start MariaDB {v}"), 0),
            (Some(v), Err(_)) => (format!("MariaDB {v}: busy…"), MF_GRAYED),
        };
        append(menu, MF_STRING | db_label.1, CMD_DB_TOGGLE, &db_label.0);
        let auto = autostart::is_enabled().unwrap_or(false);
        append(
            menu,
            MF_STRING | if auto { MF_CHECKED } else { 0 },
            CMD_AUTOSTART,
            "Start with Windows",
        );
        append(menu, MF_SEPARATOR, 0, "");
        append(menu, MF_STRING, CMD_EXIT, "Exit PHPev");

        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        // Required so the menu closes when the user clicks elsewhere.
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            pt.x,
            pt.y,
            0,
            hwnd,
            std::ptr::null(),
        ) as usize;
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu); // also destroys the submenu
        run(ctx, cmd, &site_urls);
    }
}

fn run(ctx: &Ctx, cmd: usize, site_urls: &[String]) {
    match cmd {
        0 => {}
        CMD_DASHBOARD => open_dashboard(),
        CMD_DB_TOGGLE => {
            let daemon = ctx.daemon.clone();
            ctx.rt.spawn(async move {
                let running = daemon.db.lock().await.is_some();
                let result = if running {
                    daemon.db_stop().await
                } else {
                    daemon.db_start().await
                };
                match result {
                    Ok(_) => daemon.remember_db(!running),
                    Err(e) => {
                        log_warn!("tray: MariaDB: {e}");
                        notify("MariaDB", &e.to_string());
                    }
                }
            });
        }
        CMD_AUTOSTART => {
            let on = !autostart::is_enabled().unwrap_or(false);
            if let Err(e) = autostart::set(&ctx.daemon.layout, on) {
                notify("PHPev", &format!("Could not change autostart: {e}"));
            }
        }
        CMD_EXIT => {
            let _ = ctx.daemon.shutdown.send(None);
        }
        n if n >= CMD_SITE => {
            if let Some(url) = site_urls.get(n - CMD_SITE) {
                if let Err(e) = launch::open_url(url) {
                    notify("PHPev", &format!("Could not open {url}: {e}"));
                }
            }
        }
        _ => {}
    }
}

unsafe fn append(menu: HMENU, flags: u32, id: usize, text: &str) {
    let text = wide(text);
    AppendMenuW(
        menu,
        flags,
        id,
        if flags & MF_SEPARATOR != 0 {
            std::ptr::null()
        } else {
            text.as_ptr()
        },
    );
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Copies `s` into a fixed UTF-16 buffer, truncated and NUL-terminated.
fn copy(dst: &mut [u16], s: &str) {
    let n = dst.len() - 1;
    for (d, c) in dst.iter_mut().zip(s.encode_utf16().take(n).chain(std::iter::repeat(0))) {
        *d = c;
    }
}
