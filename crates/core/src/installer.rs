//! The Windows installer (`setup.exe`, built with Inno Setup from
//! `installer\phpev.iss`): finding its entry in Settings → Apps.

use std::io;
use std::mem;
use std::path::PathBuf;

use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::registry::{self, Hive};
use crate::wide::to_wide;

/// `AppId` in `installer\phpev.iss`; Inno Setup registers the app under `<AppId>_is1`.
pub const APP_ID: &str = "{F5AFBAA9-BD8F-4024-94FF-B613075C9FB6}";

/// An installation made by `setup.exe`.
pub struct Installed {
    pub uninstaller: PathBuf,
    /// Installed for all users (Program Files) rather than only this user.
    pub all_users: bool,
}

/// The `setup.exe` installation, if PHPev was installed that way.
pub fn installed() -> Option<Installed> {
    let key = format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{APP_ID}_is1");
    [(Hive::CurrentUser, false), (Hive::LocalMachine, true)]
        .into_iter()
        .find_map(|(hive, all_users)| {
            let command = registry::read_string(hive, &key, "UninstallString").ok()??;
            let uninstaller = PathBuf::from(command.trim().trim_matches('"'));
            uninstaller.is_file().then_some(Installed { uninstaller, all_users })
        })
}

impl Installed {
    /// Starts the uninstaller (the one in Settings → Apps) without waiting.
    /// ShellExecute lets it ask for admin rights for an all-users installation.
    pub fn start_uninstaller(&self) -> io::Result<()> {
        let verb = to_wide("open");
        let file = to_wide(&self.uninstaller);
        let mut info: SHELLEXECUTEINFOW = unsafe { mem::zeroed() };
        info.cbSize = mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.nShow = SW_SHOWNORMAL;
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
