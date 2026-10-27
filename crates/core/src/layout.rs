//! On-disk layout of the PHPev home directory.

use std::io;
use std::path::{Path, PathBuf};

use crate::env_path::{self, Scope};
use crate::HOME_ENV;

/// Where the PHPev home was resolved from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HomeSource {
    /// `PHPEV_HOME` in the current process environment.
    ProcessEnv,
    /// `PHPEV_HOME` in `HKCU\Environment`, set by `phpev setup --home` (the
    /// current terminal may predate it).
    UserEnv,
    /// `%USERPROFILE%\.phpev`.
    Default,
}

#[derive(Clone, Debug)]
pub struct Layout {
    root: PathBuf,
    source: HomeSource,
}

impl Layout {
    /// Resolves the home directory: `PHPEV_HOME` (process, then user registry),
    /// falling back to `%USERPROFILE%\.phpev`.
    pub fn resolve() -> io::Result<Self> {
        if let Some(root) = std::env::var_os(HOME_ENV).filter(|v| !v.is_empty()) {
            return Ok(Self::at(PathBuf::from(root), HomeSource::ProcessEnv));
        }
        if let Some(root) = env_path::get_raw(Scope::User, HOME_ENV)?.filter(|v| !v.is_empty()) {
            return Ok(Self::at(PathBuf::from(env_path::expand(&root)), HomeSource::UserEnv));
        }
        Ok(Self::at(default_root()?, HomeSource::Default))
    }

    pub fn at(root: PathBuf, source: HomeSource) -> Self {
        Self { root, source }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn source(&self) -> HomeSource {
        self.source
    }

    pub fn shims(&self) -> PathBuf {
        self.root.join("shims")
    }

    pub fn php_dir(&self) -> PathBuf {
        self.root.join(r"bin\php")
    }

    pub fn mariadb_dir(&self) -> PathBuf {
        self.root.join(r"bin\mariadb")
    }

    pub fn composer_dir(&self) -> PathBuf {
        self.root.join(r"bin\composer")
    }

    pub fn cacert(&self) -> PathBuf {
        self.root.join(r"bin\cacert.pem")
    }

    pub fn mariadb_data_dir(&self) -> PathBuf {
        self.root.join(r"data\mariadb")
    }

    pub fn certs_dir(&self) -> PathBuf {
        self.root.join("certs")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.root.join(r"cache\downloads")
    }

    /// Edge profile of the dashboard window, apart from the user's own browsing.
    pub fn dashboard_profile_dir(&self) -> PathBuf {
        self.root.join(r"cache\dashboard")
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    /// Creates every directory of the layout (idempotent).
    pub fn create_dirs(&self) -> io::Result<()> {
        for dir in [
            self.shims(),
            self.php_dir(),
            self.mariadb_dir(),
            self.composer_dir(),
            self.mariadb_data_dir(),
            self.certs_dir(),
            self.logs_dir(),
            self.downloads_dir(),
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

/// `%USERPROFILE%\.phpev`
pub fn default_root() -> io::Result<PathBuf> {
    std::env::var_os("USERPROFILE")
        .filter(|v| !v.is_empty())
        .map(|p| PathBuf::from(p).join(".phpev"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "USERPROFILE is not set"))
}

/// The 8.3 short form of an existing path (e.g. `C:\Users\LVNDUY~1`), if the
/// volume has short names and the result is pure ASCII.
///
/// PHP cannot load `php.ini` from a non-ASCII path (spike S3), so every path
/// handed to PHP goes through this when the home is not ASCII.
pub fn ascii_short_path(path: &Path) -> Option<PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;

    let wide = crate::wide::to_wide(path);
    let needed = unsafe { GetShortPathNameW(wide.as_ptr(), std::ptr::null_mut(), 0) };
    if needed == 0 {
        return None;
    }
    let mut buf = vec![0u16; needed as usize];
    let written = unsafe { GetShortPathNameW(wide.as_ptr(), buf.as_mut_ptr(), needed) };
    if written == 0 || written >= needed {
        return None;
    }
    let short = PathBuf::from(crate::wide::from_wide(&buf));
    short.to_str().is_some_and(str::is_ascii).then_some(short)
}

/// Problems a path may cause for MariaDB / PHP extensions (spike S3).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PathRisk {
    pub non_ascii: bool,
    pub spaces: bool,
}

impl PathRisk {
    pub fn of(path: &Path) -> Self {
        let s = path.to_string_lossy();
        Self {
            non_ascii: !s.is_ascii(),
            spaces: s.contains(' '),
        }
    }

    pub fn is_clean(&self) -> bool {
        !self.non_ascii && !self.spaces
    }
}

/// Suggested home when the default one is risky.
pub const FALLBACK_HOME: &str = r"C:\PHPev";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_risk_flags_vietnamese_and_spaces() {
        assert_eq!(
            PathRisk::of(Path::new(r"C:\Users\Lê Văn Duy\.phpev")),
            PathRisk {
                non_ascii: true,
                spaces: true
            }
        );
        assert!(PathRisk::of(Path::new(r"C:\Users\LE_VAN_DUY\.phpev")).is_clean());
    }

    #[test]
    fn short_path_is_ascii_and_points_to_same_dir() {
        let dir = std::env::temp_dir().join(format!("phpev Lê Văn {}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Volumes without 8.3 names legitimately return None.
        if let Some(short) = ascii_short_path(&dir) {
            assert!(short.to_str().unwrap().is_ascii());
            assert!(short.is_dir());
        }
        assert_eq!(ascii_short_path(&dir.join("missing")), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
