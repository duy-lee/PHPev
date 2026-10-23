//! Reading and editing the persistent `PATH` (and other environment variables)
//! stored in the registry.
//!
//! Windows builds a new process's `PATH` as `System PATH` followed by
//! `User PATH`, so a directory in the user PATH can never shadow one in the
//! system PATH. See DESIGN_SPEC §5.

use std::io;
use std::path::{Path, PathBuf};
use std::ptr;

use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
};

use crate::registry::{self, Hive};
use crate::wide::{from_wide, to_wide};

/// Which persistent environment block to operate on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// `HKCU\Environment` — writable without admin rights.
    User,
    /// `HKLM\...\Session Manager\Environment` — needs admin rights to write.
    System,
}

impl Scope {
    fn key(self) -> (Hive, &'static str) {
        match self {
            Scope::User => (Hive::CurrentUser, "Environment"),
            Scope::System => (
                Hive::LocalMachine,
                r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
            ),
        }
    }
}

/// Reads a variable without expanding `%VARS%`, so it can be written back unchanged.
/// Returns `Ok(None)` if the variable does not exist.
pub fn get_raw(scope: Scope, name: &str) -> io::Result<Option<String>> {
    let (hkey, subkey) = scope.key();
    registry::read_string(hkey, subkey, name)
}

/// Writes a variable as `REG_EXPAND_SZ` so `%VARS%` inside it keep working.
pub fn set_raw(scope: Scope, name: &str, value: &str) -> io::Result<()> {
    let (hkey, subkey) = scope.key();
    registry::write_expand_string(hkey, subkey, name, value)
}

/// Tells Explorer (and anything else listening) that the environment changed,
/// so terminals opened afterwards see the new values.
pub fn broadcast_change() {
    let param = to_wide("Environment");
    let mut result = 0usize;
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            param.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            2000,
            &mut result,
        );
    }
}

/// Expands `%VARS%` using the current process environment.
pub fn expand(s: &str) -> String {
    let src = to_wide(s);
    let needed = unsafe { ExpandEnvironmentStringsW(src.as_ptr(), ptr::null_mut(), 0) };
    if needed == 0 {
        return s.to_owned();
    }
    let mut buf = vec![0u16; needed as usize];
    let written = unsafe { ExpandEnvironmentStringsW(src.as_ptr(), buf.as_mut_ptr(), needed) };
    if written == 0 || written > needed {
        return s.to_owned();
    }
    from_wide(&buf).to_string_lossy().into_owned()
}

/// Splits a `PATH` value into its non-empty entries.
pub fn split(raw: &str) -> Vec<&str> {
    raw.split(';').map(str::trim).filter(|s| !s.is_empty()).collect()
}

/// Compares two PATH entries the way Windows does: case-insensitive, ignoring
/// trailing separators and surrounding quotes.
pub fn same_dir(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> String {
        expand(s.trim().trim_matches('"'))
            .trim_end_matches(['\\', '/'])
            .replace('/', "\\")
            .to_lowercase()
    }
    norm(a) == norm(b)
}

/// Returns `raw` with `dir` moved (or added) to the front, without duplicates.
pub fn prepend(raw: &str, dir: &str) -> String {
    let mut parts = vec![dir];
    parts.extend(split(raw).into_iter().filter(|p| !same_dir(p, dir)));
    parts.join(";")
}

/// Position of `dir` within a PATH value, if present.
pub fn position(raw: &str, dir: &str) -> Option<usize> {
    split(raw).iter().position(|p| same_dir(p, dir))
}

/// The PATH a newly opened terminal will get: system entries, then user entries,
/// with `%VARS%` expanded.
pub fn effective_path() -> io::Result<Vec<(Scope, PathBuf)>> {
    let mut out = Vec::new();
    for scope in [Scope::System, Scope::User] {
        if let Some(raw) = get_raw(scope, "Path")? {
            out.extend(
                split(&raw)
                    .into_iter()
                    .map(|p| (scope, PathBuf::from(expand(p.trim_matches('"'))))),
            );
        }
    }
    Ok(out)
}

/// Finds the first directory on `path` that contains any of `names`,
/// i.e. what a terminal would run for that command.
pub fn which<'a>(path: &'a [(Scope, PathBuf)], names: &[&str]) -> Option<(&'a Scope, PathBuf)> {
    path.iter().find_map(|(scope, dir)| {
        names
            .iter()
            .map(|n| dir.join(n))
            .find(|p| p.is_file())
            .map(|p| (scope, p))
    })
}

/// All entries on `path` providing any of `names`, in resolution order.
pub fn which_all(path: &[(Scope, PathBuf)], names: &[&str]) -> Vec<(Scope, PathBuf)> {
    path.iter()
        .flat_map(|(scope, dir)| names.iter().map(move |n| (*scope, dir.join(n))))
        .filter(|(_, p)| p.is_file())
        .collect()
}

/// True if `file` lives directly inside `dir`.
pub fn is_in_dir(file: &Path, dir: &Path) -> bool {
    file.parent()
        .is_some_and(|p| same_dir(&p.to_string_lossy(), &dir.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepend_moves_existing_entry_to_front() {
        let raw = r"C:\a;C:\Shims\;C:\b";
        assert_eq!(prepend(raw, r"C:\shims"), r"C:\shims;C:\a;C:\b");
    }

    #[test]
    fn prepend_skips_empty_entries() {
        assert_eq!(prepend(";C:\\a;;", r"C:\s"), r"C:\s;C:\a");
    }

    #[test]
    fn same_dir_ignores_case_quotes_and_trailing_slash() {
        assert!(same_dir(r#""C:\Tools\Bin\""#, r"c:/tools/bin"));
        assert!(!same_dir(r"C:\tools\bin", r"C:\tools\bin2"));
    }
}
