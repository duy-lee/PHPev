//! PHP version resolution for the `php.exe` / `composer.exe` shims, and the
//! MariaDB client for the `mysql.exe` / `mysqldump.exe` ones.
//!
//! Runs on every `php` invocation, so it avoids TOML and the registry: the home
//! is the parent of the shims directory, installed versions come from one
//! `read_dir`, and config-derived data comes from the flat `shims.cache` that
//! is rewritten whenever config.toml is saved.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::version;

pub const CACHE_FILE: &str = "shims.cache";
pub const VERSION_FILE: &str = ".php-version";

/// Where the wanted version came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    VersionFile(PathBuf),
    Site(PathBuf),
    Default,
    /// No preference anywhere: the newest installed version.
    Newest,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::VersionFile(p) => write!(f, "{}", p.display()),
            Source::Site(p) => write!(f, "linked site {}", p.display()),
            Source::Default => f.write_str("default (phpev php use)"),
            Source::Newest => f.write_str("newest installed"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub wanted: String,
    /// Installed version directory name, e.g. `8.4.27`.
    pub version: String,
    pub dir: PathBuf,
    pub source: Source,
}

#[derive(Debug)]
pub enum Error {
    NothingInstalled,
    NotInstalled { wanted: String, source: Source },
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NothingInstalled => f.write_str("no PHP version is installed. Run: phpev php install 8.4"),
            Error::NotInstalled { wanted, source } => write!(
                f,
                "PHP {wanted} (from {source}) is not installed. Run: phpev php install {wanted}"
            ),
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

/// Picks the PHP version for a command run in `cwd`:
/// `.php-version` (cwd or nearest parent) → linked site containing `cwd` →
/// default → newest installed.
pub fn resolve(home: &Path, cwd: &Path) -> Result<Resolved, Error> {
    let php_dir = home.join(r"bin\php");
    let installed = installed_versions(&php_dir);
    if installed.is_empty() {
        return Err(Error::NothingInstalled);
    }

    let cache = std::fs::read_to_string(home.join(CACHE_FILE)).unwrap_or_default();
    let cache = Cache::parse(&cache);
    let (wanted, source) = if let Some((file, wanted)) = find_version_file(cwd) {
        (wanted, Source::VersionFile(file))
    } else if let Some((site, wanted)) = cache.site_for(cwd) {
        (wanted.to_owned(), Source::Site(site.to_owned()))
    } else if let Some(default) = cache.default {
        (default.to_owned(), Source::Default)
    } else {
        (installed[0].clone(), Source::Newest)
    };

    match version::best_match(&wanted, installed.iter().map(String::as_str)) {
        Some(v) => Ok(Resolved {
            version: v.to_owned(),
            dir: php_dir.join(v),
            wanted,
            source,
        }),
        None => Err(Error::NotInstalled { wanted, source }),
    }
}

/// The MariaDB whose client tools the `mysql` / `mysqldump` shims run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MariaDb {
    pub version: String,
    /// Its `bin` folder (mariadb.exe, mariadb-dump.exe).
    pub bin: PathBuf,
    pub port: u16,
}

/// The active MariaDB (`phpev db use`), or the newest installed one.
pub fn resolve_mariadb(home: &Path) -> Result<MariaDb, String> {
    let dir = home.join(r"bin\mariadb");
    let mut installed: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| version::parts(name).is_some() && dir.join(name).join(r"bin\mariadb.exe").is_file())
        .collect();
    installed.sort_by(|a, b| version::cmp(b, a));
    if installed.is_empty() {
        return Err("MariaDB is not installed. Run: phpev db install 11.4".into());
    }
    let cache = std::fs::read_to_string(home.join(CACHE_FILE)).unwrap_or_default();
    let cache = Cache::parse(&cache);
    let version = match cache.mariadb {
        Some(major) => version::best_match(major, installed.iter().map(String::as_str))
            .ok_or_else(|| format!("MariaDB {major} (phpev db use) is not installed. Run: phpev db install {major}"))?,
        None => &installed[0],
    };
    Ok(MariaDb {
        bin: dir.join(version).join("bin"),
        version: version.to_owned(),
        port: cache.mariadb_port.unwrap_or(3306),
    })
}

/// Version directory names under `bin\php` that contain php.exe, newest first.
fn installed_versions(php_dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(php_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| version::parts(name).is_some() && php_dir.join(name).join("php.exe").is_file())
        .collect();
    v.sort_by(|a, b| version::cmp(b, a));
    v
}

/// Nearest `.php-version` at or above `dir`, with its normalized content.
pub fn find_version_file(dir: &Path) -> Option<(PathBuf, String)> {
    dir.ancestors().find_map(|d| {
        let file = d.join(VERSION_FILE);
        let text = std::fs::read_to_string(&file).ok()?;
        parse_version_file(&text).map(|v| (file, v))
    })
}

/// Accepts `8.4`, `8.4.27`, `php-8.4`, `php8.4`, surrounding whitespace and comments.
fn parse_version_file(text: &str) -> Option<String> {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))?;
    let v = line.trim_start_matches("php").trim_start_matches(['-', '@']).trim();
    version::parts(v).is_some().then(|| v.to_owned())
}

/// Parsed `shims.cache`: tab-separated `default <ver>`, `site <path> <ver>`,
/// `mariadb <major>` and `mariadb-port <port>` lines.
#[derive(Debug, Default)]
struct Cache<'a> {
    default: Option<&'a str>,
    sites: Vec<(&'a Path, &'a str)>,
    mariadb: Option<&'a str>,
    mariadb_port: Option<u16>,
}

impl<'a> Cache<'a> {
    fn parse(text: &'a str) -> Self {
        let mut c = Cache::default();
        for line in text.lines() {
            let mut f = line.split('\t');
            match (f.next(), f.next(), f.next()) {
                (Some("default"), Some(v), None) => c.default = Some(v),
                (Some("site"), Some(p), Some(v)) => c.sites.push((Path::new(p), v)),
                (Some("mariadb"), Some(v), None) => c.mariadb = Some(v),
                (Some("mariadb-port"), Some(p), None) => c.mariadb_port = p.parse().ok(),
                _ => {}
            }
        }
        c
    }

    /// The most specific (longest) site path containing `cwd`.
    fn site_for(&self, cwd: &Path) -> Option<(&'a Path, &'a str)> {
        self.sites
            .iter()
            .filter(|(p, _)| path_starts_with(cwd, p))
            .max_by_key(|(p, _)| p.components().count())
            .copied()
    }
}

/// Case-insensitive, component-wise prefix test (Windows paths).
fn path_starts_with(path: &Path, prefix: &Path) -> bool {
    let mut a = path.components();
    prefix.components().all(|pc| {
        a.next().is_some_and(|ac| {
            ac.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&pc.as_os_str().to_string_lossy())
        })
    })
}

/// Writes `shims.cache` next to config.toml.
pub fn write_cache(config_file: &Path, config: &Config) -> io::Result<()> {
    let mut text = String::from("# Generated by phpev from config.toml for the shims. Do not edit.\n");
    if let Some(php) = &config.php {
        text.push_str(&format!("default\t{php}\n"));
    }
    for site in &config.sites {
        if let Some(php) = &site.php {
            text.push_str(&format!("site\t{}\t{php}\n", site.path.display()));
        }
    }
    if let Some(major) = &config.mariadb {
        text.push_str(&format!("mariadb\t{major}\n"));
    }
    text.push_str(&format!("mariadb-port\t{}\n", config.ports.mariadb));
    crate::config::write_atomic(&config_file.with_file_name(CACHE_FILE), text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Site;

    fn home(name: &str, versions: &[&str]) -> PathBuf {
        let h = std::env::temp_dir().join(format!("phpev-shim-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&h);
        for v in versions {
            let d = h.join(r"bin\php").join(v);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("php.exe"), "").unwrap();
        }
        std::fs::create_dir_all(&h).unwrap();
        h
    }

    #[test]
    fn version_file_formats() {
        assert_eq!(parse_version_file("8.4\n").as_deref(), Some("8.4"));
        assert_eq!(
            parse_version_file("# comment\n  php-8.3.36 ").as_deref(),
            Some("8.3.36")
        );
        assert_eq!(parse_version_file("php8.2").as_deref(), Some("8.2"));
        assert_eq!(parse_version_file("latest"), None);
    }

    #[test]
    fn precedence_version_file_then_site_then_default_then_newest() {
        let h = home("prec", &["8.3.36", "8.4.27", "8.2.35"]);
        let project = h.join(r"code\blog\app");
        std::fs::create_dir_all(&project).unwrap();

        // Nothing configured: newest installed.
        let r = resolve(&h, &project).unwrap();
        assert_eq!((r.version.as_str(), &r.source), ("8.4.27", &Source::Newest));

        // Default from config.
        let mut cfg = Config {
            php: Some("8.3".into()),
            ..Config::default()
        };
        write_cache(&h.join("config.toml"), &cfg).unwrap();
        assert_eq!(resolve(&h, &project).unwrap().version, "8.3.36");

        // A linked site containing cwd wins over the default (case-insensitive).
        cfg.sites.push(Site {
            name: "blog".into(),
            path: h.join(r"CODE\Blog"),
            php: Some("8.2".into()),
            secure: false,
            defender_excluded: false,
        });
        write_cache(&h.join("config.toml"), &cfg).unwrap();
        let r = resolve(&h, &project).unwrap();
        assert_eq!(r.version, "8.2.35");
        assert!(matches!(r.source, Source::Site(_)));

        // .php-version in a parent wins over everything.
        std::fs::write(h.join(r"code\blog\.php-version"), "8.4\n").unwrap();
        let r = resolve(&h, &project).unwrap();
        assert_eq!(r.version, "8.4.27");
        assert_eq!(r.source, Source::VersionFile(h.join(r"code\blog\.php-version")));
        assert_eq!(r.dir, h.join(r"bin\php\8.4.27"));

        std::fs::remove_dir_all(&h).unwrap();
    }

    #[test]
    fn missing_version_is_reported_with_its_source() {
        let h = home("missing", &["8.4.27"]);
        std::fs::write(h.join(VERSION_FILE), "7.4").unwrap();
        match resolve(&h, &h) {
            Err(Error::NotInstalled { wanted, source }) => {
                assert_eq!(wanted, "7.4");
                assert_eq!(source, Source::VersionFile(h.join(VERSION_FILE)));
            }
            other => panic!("{other:?}"),
        }
        std::fs::remove_dir_all(&h).unwrap();
    }

    #[test]
    fn mariadb_client_follows_the_active_major_and_port() {
        let h = home("mariadb", &[]);
        assert!(resolve_mariadb(&h).unwrap_err().contains("not installed"));
        for v in ["11.4.13", "10.11.14"] {
            let bin = h.join(r"bin\mariadb").join(v).join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            std::fs::write(bin.join("mariadb.exe"), "").unwrap();
        }
        // No cache yet: newest installed, default port.
        let m = resolve_mariadb(&h).unwrap();
        assert_eq!((m.version.as_str(), m.port), ("11.4.13", 3306));

        let mut cfg = Config {
            mariadb: Some("10.11".into()),
            ..Config::default()
        };
        cfg.ports.mariadb = 3307;
        write_cache(&h.join("config.toml"), &cfg).unwrap();
        let m = resolve_mariadb(&h).unwrap();
        assert_eq!((m.version.as_str(), m.port), ("10.11.14", 3307));
        assert_eq!(m.bin, h.join(r"bin\mariadb\10.11.14\bin"));
        std::fs::remove_dir_all(&h).unwrap();
    }

    #[test]
    fn nothing_installed() {
        let h = home("none", &[]);
        assert!(matches!(resolve(&h, &h), Err(Error::NothingInstalled)));
        std::fs::remove_dir_all(&h).unwrap();
    }
}
