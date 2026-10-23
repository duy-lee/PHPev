//! Clean shutdown on log off / Windows shutdown (DESIGN_SPEC §10).
//!
//! A GUI-subsystem process gets no console CTRL_SHUTDOWN_EVENT; Windows instead
//! sends WM_QUERYENDSESSION / WM_ENDSESSION to top-level windows. A hidden
//! window on its own thread receives them and blocks WM_ENDSESSION until the
//! services are stopped (so MariaDB flushes instead of needing crash recovery).

use std::sync::mpsc as std_mpsc;
use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::mpsc::UnboundedSender;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW, TranslateMessage, MSG,
    WM_ENDSESSION, WM_QUERYENDSESSION, WNDCLASSW, WS_OVERLAPPED,
};

use crate::log::{log_info, log_warn};

/// What the daemon receives: a sender to acknowledge once everything is stopped.
pub type ShutdownRequest = Option<std_mpsc::Sender<()>>;

static SHUTDOWN: OnceLock<UnboundedSender<ShutdownRequest>> = OnceLock::new();

/// How long Windows' end-session handling is held up waiting for the services.
const MAX_WAIT: Duration = Duration::from_secs(20);

pub fn spawn(shutdown: UnboundedSender<ShutdownRequest>) {
    let _ = SHUTDOWN.set(shutdown);
    let spawned = std::thread::Builder::new().name("endsession".into()).spawn(|| unsafe {
        let class: Vec<u16> = "PHPevDaemonEndSession\0".encode_utf16().collect();
        let instance = GetModuleHandleW(std::ptr::null());
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.lpfnWndProc = Some(wndproc);
        wc.hInstance = instance;
        wc.lpszClassName = class.as_ptr();
        RegisterClassW(&wc);
        // A real (never shown) top-level window: message-only windows do not
        // receive the end-session broadcast.
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            log_warn!("end-session window could not be created; services will not stop cleanly at log off");
            return;
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
    if let Err(e) = spawned {
        log_warn!("end-session thread failed to start: {e}");
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_QUERYENDSESSION => 1,
        WM_ENDSESSION if wparam != 0 => {
            log_info!("Windows is ending the session; stopping services");
            if let Some(tx) = SHUTDOWN.get() {
                let (ack_tx, ack_rx) = std_mpsc::channel();
                if tx.send(Some(ack_tx)).is_ok() {
                    let _ = ack_rx.recv_timeout(MAX_WAIT);
                }
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
