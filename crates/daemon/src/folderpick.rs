//! The Windows "Select Folder" dialog, for the dashboard (a web page cannot
//! learn the full path of a folder the user picks).
//!
//! windows-sys has no COM interface wrappers, so the few IFileOpenDialog and
//! IShellItem methods needed are called through their vtables.

use std::ffi::c_void;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows_sys::core::{GUID, HRESULT};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    COINIT_DISABLE_OLE1DDE,
};
use windows_sys::Win32::System::Threading::AttachThreadInput;
use windows_sys::Win32::UI::Shell::{FileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, FindWindowW, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
};

const IID_IFILEOPENDIALOG: GUID = GUID::from_u128(0xd57c7288_d4ad_4768_be02_9d969532d960);
/// HRESULT_FROM_WIN32(ERROR_CANCELLED): the user closed the dialog.
const CANCELLED: HRESULT = 0x8007_04C7_u32 as i32;

// Vtable slots: IUnknown 0-2, IModalWindow::Show 3, IFileDialog 4-26, IShellItem 3-7.
const RELEASE: usize = 2;
const SHOW: usize = 3;
const SET_OPTIONS: usize = 9;
const GET_OPTIONS: usize = 10;
const SET_TITLE: usize = 17;
const GET_RESULT: usize = 20;
const GET_DISPLAY_NAME: usize = 5;

static OPEN: AtomicBool = AtomicBool::new(false);

/// Shows the dialog over `owner` (the dashboard window, if found) and waits.
/// Returns the chosen folder, or `None` if the user cancelled.
pub fn pick_folder(title: &str, owner: Option<HWND>) -> io::Result<Option<String>> {
    if OPEN.swap(true, Ordering::SeqCst) {
        return Err(io::Error::other("a folder dialog is already open"));
    }
    let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let owner = owner.map_or(0, |h| h as isize);
    // COM needs a single-threaded apartment of its own for the modal dialog.
    let shown = {
        let title = title.clone();
        std::thread::spawn(move || unsafe { show(&title, owner as HWND) })
    };
    bring_to_front(&title);
    let result = shown
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("the folder dialog crashed")));
    OPEN.store(false, Ordering::SeqCst);
    result
}

/// The daemon runs in the background, so Windows does not let its dialog take
/// the focus from the dashboard the user just clicked in: it would open
/// behind. Sharing input with the foreground window's thread for a moment
/// lifts that restriction.
fn bring_to_front(title: &[u16]) {
    let class: Vec<u16> = "#32770".encode_utf16().chain(Some(0)).collect();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let dialog = unsafe { FindWindowW(class.as_ptr(), title.as_ptr()) };
        let mut pid = 0;
        let thread = unsafe { GetWindowThreadProcessId(dialog, &mut pid) };
        if !dialog.is_null() && pid == std::process::id() {
            unsafe {
                let fg_thread = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
                let attached = fg_thread != 0 && fg_thread != thread && AttachThreadInput(thread, fg_thread, 1) != 0;
                SetForegroundWindow(dialog);
                BringWindowToTop(dialog);
                if attached {
                    AttachThreadInput(thread, fg_thread, 0);
                }
            }
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

unsafe fn show(title: &[u16], owner: HWND) -> io::Result<Option<String>> {
    check(CoInitializeEx(
        std::ptr::null(),
        (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
    ))?;
    let result = (|| {
        let mut dialog: *mut c_void = std::ptr::null_mut();
        check(CoCreateInstance(
            &FileOpenDialog,
            std::ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &IID_IFILEOPENDIALOG,
            &mut dialog,
        ))?;
        let picked = run(dialog, title, owner);
        call::<unsafe extern "system" fn(*mut c_void) -> u32>(dialog, RELEASE)(dialog);
        picked
    })();
    CoUninitialize();
    result
}

unsafe fn run(dialog: *mut c_void, title: &[u16], owner: HWND) -> io::Result<Option<String>> {
    let mut options = 0u32;
    check(call::<unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT>(
        dialog,
        GET_OPTIONS,
    )(dialog, &mut options))?;
    check(call::<unsafe extern "system" fn(*mut c_void, u32) -> HRESULT>(
        dialog,
        SET_OPTIONS,
    )(dialog, options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM))?;
    check(call::<unsafe extern "system" fn(*mut c_void, *const u16) -> HRESULT>(
        dialog, SET_TITLE,
    )(dialog, title.as_ptr()))?;
    match call::<unsafe extern "system" fn(*mut c_void, HWND) -> HRESULT>(dialog, SHOW)(dialog, owner) {
        CANCELLED => return Ok(None),
        hr => check(hr)?,
    }
    let mut item: *mut c_void = std::ptr::null_mut();
    check(call::<
        unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
    >(dialog, GET_RESULT)(dialog, &mut item))?;
    let mut name: *mut u16 = std::ptr::null_mut();
    let hr = call::<unsafe extern "system" fn(*mut c_void, i32, *mut *mut u16) -> HRESULT>(item, GET_DISPLAY_NAME)(
        item,
        SIGDN_FILESYSPATH,
        &mut name,
    );
    call::<unsafe extern "system" fn(*mut c_void) -> u32>(item, RELEASE)(item);
    check(hr)?;
    let len = (0..).take_while(|&i| *name.add(i) != 0).count();
    let path = String::from_utf16_lossy(std::slice::from_raw_parts(name, len));
    CoTaskMemFree(name as *const c_void);
    Ok(Some(path))
}

/// The method in vtable slot `slot` of the COM object `obj`, as type `F`.
unsafe fn call<F: Copy>(obj: *mut c_void, slot: usize) -> F {
    let vtable = *(obj as *const *const usize);
    let f = *vtable.add(slot);
    std::mem::transmute_copy(&f)
}

fn check(hr: HRESULT) -> io::Result<()> {
    if hr >= 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(hr))
    }
}
