//! Puts the daemon in a kill-on-close Job Object (DESIGN_SPEC §0 #7).
//!
//! Children inherit job membership at creation, so there is no window in which
//! a freshly spawned php-cgi/mariadbd is outside the job. The daemon holds the
//! only handle: when it exits — cleanly or killed — Windows closes the handle
//! and terminates every process still in the job. No orphans keep ports busy.

use std::io;
use std::mem;

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_BREAKAWAY_OK, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

/// Creates the job and assigns the current process to it. The handle is
/// intentionally leaked: it must live exactly as long as the process.
pub fn enter_kill_on_close_job() -> io::Result<()> {
    unsafe {
        let job: HANDLE = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = mem::zeroed();
        // BREAKAWAY_OK: windows the user asked for (Explorer, Terminal, browser)
        // are started outside the job (CREATE_BREAKAWAY_FROM_JOB) and must not
        // close when PHPev stops. Supervised children still inherit the job.
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        if AssignProcessToJobObject(job, GetCurrentProcess()) == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
