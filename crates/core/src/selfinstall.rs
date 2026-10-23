//! Copies the PHPev executables and shims into `<home>\shims`, the single
//! directory PHPev puts on PATH.

use std::io;
use std::path::Path;

use crate::layout::Layout;

/// Shim binary as built, and the names it is installed under.
const SHIM_SOURCE: &str = "phpev-shim.exe";
const SHIM_NAMES: &[&str] = &["php.exe", "composer.exe"];
/// PHPev's own executables, installed under their own names.
const TOOLS: &[&str] = &["phpev.exe", "phpev-elevate.exe", "phpev-daemon.exe"];

/// Installs from `src_dir` (normally the directory of the running phpev.exe).
/// Returns the names that were added or updated.
pub fn install(layout: &Layout, src_dir: &Path) -> io::Result<Vec<&'static str>> {
    let dest_dir = layout.shims();
    std::fs::create_dir_all(&dest_dir)?;
    let mut changed = Vec::new();

    let pairs = TOOLS
        .iter()
        .map(|t| (*t, *t))
        .chain(SHIM_NAMES.iter().map(|n| (SHIM_SOURCE, *n)));
    for (src_name, dest_name) in pairs {
        let src = src_dir.join(src_name);
        if !src.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} is missing next to phpev.exe", src.display()),
            ));
        }
        if replace_if_different(&src, &dest_dir.join(dest_name))? {
            changed.push(dest_name);
        }
    }
    Ok(changed)
}

/// Copies `src` over `dest` unless the contents are identical.
///
/// A running executable cannot be overwritten on Windows but can be renamed,
/// so the old file is moved to `*.old` first; it is deleted now if possible,
/// otherwise on the next install (§10, file locking).
fn replace_if_different(src: &Path, dest: &Path) -> io::Result<bool> {
    let old = dest.with_extension("exe.old");
    let _ = std::fs::remove_file(&old);

    if dest.is_file() && std::fs::read(src)? == std::fs::read(dest)? {
        return Ok(false);
    }
    if dest.exists() {
        std::fs::rename(dest, &old)?;
    }
    if let Err(e) = std::fs::copy(src, dest) {
        let _ = std::fs::rename(&old, dest); // put the previous version back
        return Err(e);
    }
    let _ = std::fs::remove_file(&old);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_only_when_changed_and_survives_locked_target() {
        let dir = std::env::temp_dir().join(format!("phpev-selfinstall-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (src, dest) = (dir.join("src.exe"), dir.join("dest.exe"));

        std::fs::write(&src, "v1").unwrap();
        assert!(replace_if_different(&src, &dest).unwrap());
        assert!(!replace_if_different(&src, &dest).unwrap());

        // Simulate a running exe: keep dest open (Rust opens with FILE_SHARE_DELETE, so rename works).
        let _held = std::fs::File::open(&dest).unwrap();
        std::fs::write(&src, "v2").unwrap();
        assert!(replace_if_different(&src, &dest).unwrap());
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "v2");

        drop(_held);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
