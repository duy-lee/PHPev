//! The Windows file dialogs (select a folder, open a file, save a file), for
//! the dashboard: a web page cannot learn the full path the user picks.
//!
//! windows-sys has no COM interface wrappers, so the few IFileDialog and
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
use windows_sys::Win32::UI::Shell::{
    FileOpenDialog, FileSaveDialog, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT, FOS_PICKFOLDERS,
    SIGDN_FILESYSPATH,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId};

const IID_IFILEOPENDIALOG: GUID = GUID::from_u128(0xd57c7288_d4ad_4768_be02_9d969532d960);
const IID_IFILESAVEDIALOG: GUID = GUID::from_u128(0x84bccd23_5fde_4cdb_aea4_af64b83d78ab);
/// HRESULT_FROM_WIN32(ERROR_CANCELLED): the user closed the dialog.
const CANCELLED: HRESULT = 0x8007_04C7_u32 as i32;

// Vtable slots: IUnknown 0-2, IModalWindow::Show 3, IFileDialog 4-26, IShellItem 3-7.
const RELEASE: usize = 2;
const SHOW: usize = 3;
const SET_FILE_TYPES: usize = 4;
const SET_OPTIONS: usize = 9;
const GET_OPTIONS: usize = 10;
const SET_FILE_NAME: usize = 15;
const SET_TITLE: usize = 17;
const GET_RESULT: usize = 20;
const SET_DEFAULT_EXTENSION: usize = 22;
const GET_DISPLAY_NAME: usize = 5;

/// COMDLG_FILTERSPEC
#[repr(C)]
struct FilterSpec {
    name: *const u16,
    spec: *const u16,
}

static OPEN: AtomicBool = AtomicBool::new(false);

enum Kind {
    Folder,
    /// (filter name, filter pattern such as `*.sql`)
    Open(String, String),
    /// (filter name, filter pattern, proposed file name)
    Save(String, String, String),
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Shows the "Select Folder" dialog over `owner` (the dashboard window, if
/// found) and waits. Returns the chosen folder, or `None` if cancelled.
pub fn pick_folder(title: &str, owner: Option<HWND>) -> io::Result<Option<String>> {
    run_dialog(Kind::Folder, title, owner)
}

/// "Open" dialog for one file of type `pattern` (e.g. `*.sql`).
pub fn pick_file(title: &str, owner: Option<HWND>, filter_name: &str, pattern: &str) -> io::Result<Option<String>> {
    run_dialog(Kind::Open(filter_name.into(), pattern.into()), title, owner)
}

/// "Save as" dialog proposing `file_name`; asks before replacing a file.
pub fn save_file(
    title: &str,
    owner: Option<HWND>,
    filter_name: &str,
    pattern: &str,
    file_name: &str,
) -> io::Result<Option<String>> {
    run_dialog(
        Kind::Save(filter_name.into(), pattern.into(), file_name.into()),
        title,
        owner,
    )
}

fn run_dialog(kind: Kind, title: &str, owner: Option<HWND>) -> io::Result<Option<String>> {
    if OPEN.swap(true, Ordering::SeqCst) {
        return Err(io::Error::other("a file dialog is already open"));
    }
    let title = wide(title);
    let owner = owner.map_or(0, |h| h as isize);
    // COM needs a single-threaded apartment of its own for the modal dialog.
    let shown = {
        let title = title.clone();
        std::thread::spawn(move || unsafe { show(&kind, &title, owner as HWND) })
    };
    bring_to_front(&title);
    let result = shown
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("the file dialog crashed")));
    OPEN.store(false, Ordering::SeqCst);
    result
}

/// Brings the dialog in front of the dashboard as soon as it appears.
fn bring_to_front(title: &[u16]) {
    let class = wide("#32770");
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let dialog = unsafe { FindWindowW(class.as_ptr(), title.as_ptr()) };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(dialog, &mut pid) };
        if !dialog.is_null() && pid == std::process::id() {
            crate::foreground::force(dialog);
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

unsafe fn show(kind: &Kind, title: &[u16], owner: HWND) -> io::Result<Option<String>> {
    check(CoInitializeEx(
        std::ptr::null(),
        (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
    ))?;
    let result = (|| {
        let (clsid, iid) = match kind {
            Kind::Save(..) => (&FileSaveDialog, &IID_IFILESAVEDIALOG),
            _ => (&FileOpenDialog, &IID_IFILEOPENDIALOG),
        };
        let mut dialog: *mut c_void = std::ptr::null_mut();
        check(CoCreateInstance(
            clsid,
            std::ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            iid,
            &mut dialog,
        ))?;
        let picked = run(dialog, kind, title, owner);
        call::<unsafe extern "system" fn(*mut c_void) -> u32>(dialog, RELEASE)(dialog);
        picked
    })();
    CoUninitialize();
    result
}

unsafe fn run(dialog: *mut c_void, kind: &Kind, title: &[u16], owner: HWND) -> io::Result<Option<String>> {
    let mut options = 0u32;
    check(call::<unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT>(
        dialog,
        GET_OPTIONS,
    )(dialog, &mut options))?;
    options |= FOS_FORCEFILESYSTEM
        | match kind {
            Kind::Folder => FOS_PICKFOLDERS,
            Kind::Open(..) => FOS_FILEMUSTEXIST,
            Kind::Save(..) => FOS_OVERWRITEPROMPT,
        };
    check(call::<unsafe extern "system" fn(*mut c_void, u32) -> HRESULT>(
        dialog,
        SET_OPTIONS,
    )(dialog, options))?;
    check(call::<unsafe extern "system" fn(*mut c_void, *const u16) -> HRESULT>(
        dialog, SET_TITLE,
    )(dialog, title.as_ptr()))?;

    // Kept alive until the dialog has closed: the dialog reads them while open.
    let (filter_name, pattern, file_name, extension);
    if let Kind::Open(name, spec) | Kind::Save(name, spec, _) = kind {
        filter_name = wide(name);
        pattern = wide(spec);
        let filters = [FilterSpec {
            name: filter_name.as_ptr(),
            spec: pattern.as_ptr(),
        }];
        check(call::<
            unsafe extern "system" fn(*mut c_void, u32, *const FilterSpec) -> HRESULT,
        >(dialog, SET_FILE_TYPES)(dialog, 1, filters.as_ptr()))?;
    }
    if let Kind::Save(_, spec, proposed) = kind {
        file_name = wide(proposed);
        extension = wide(spec.trim_start_matches("*."));
        check(call::<unsafe extern "system" fn(*mut c_void, *const u16) -> HRESULT>(
            dialog,
            SET_FILE_NAME,
        )(dialog, file_name.as_ptr()))?;
        check(call::<unsafe extern "system" fn(*mut c_void, *const u16) -> HRESULT>(
            dialog,
            SET_DEFAULT_EXTENSION,
        )(dialog, extension.as_ptr()))?;
    }

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
