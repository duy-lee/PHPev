//! Site table: linked sites plus the sub-folders of parked directories
//! (DESIGN_SPEC §4, decided 2026-10-23).

use std::path::{Path, PathBuf};

use crate::config::Config;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Linked,
    Parked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Site {
    /// DNS label, e.g. `my-app` for `my-app.localhost`.
    pub name: String,
    pub path: PathBuf,
    pub kind: Kind,
    /// PHP version pinned in config (linked sites only); `.php-version` still wins.
    pub php: Option<String>,
}

impl Site {
    /// `public/` when it holds a front controller (Laravel, Symfony, Slim…),
    /// otherwise the project folder itself (plain PHP).
    pub fn docroot(&self) -> PathBuf {
        let public = self.path.join("public");
        if public.join("index.php").is_file() {
            public
        } else {
            self.path.clone()
        }
    }

    pub fn host(&self, tld: &str) -> String {
        format!("{}.{tld}", self.name)
    }
}

/// Folder name → DNS label: lowercase, accents removed (`Dự án` → `du-an`),
/// anything else outside `a-z0-9` becomes `-`, runs of `-` collapse, no
/// leading/trailing `-`. `None` if nothing is left.
pub fn normalize_name(folder: &str) -> Option<String> {
    let mut out = String::with_capacity(folder.len());
    for c in folder.chars().flat_map(char::to_lowercase).map(fold_accent) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let name = out.trim_matches('-');
    (!name.is_empty() && name.len() <= 63).then(|| name.to_owned())
}

/// Base letter of an accented Vietnamese / Latin-1 letter (lowercase input).
fn fold_accent(c: char) -> char {
    const GROUPS: &[(&str, char)] = &[
        ("àáảãạăằắẳẵặâầấẩẫậäå", 'a'),
        ("èéẻẽẹêềếểễệë", 'e'),
        ("ìíỉĩịîï", 'i'),
        ("òóỏõọôồốổỗộơờớởỡợöø", 'o'),
        ("ùúủũụưừứửữựûü", 'u'),
        ("ỳýỷỹỵÿ", 'y'),
        ("đ", 'd'),
        ("ç", 'c'),
        ("ñ", 'n'),
    ];
    if c.is_ascii() {
        return c;
    }
    GROUPS
        .iter()
        .find(|(set, _)| set.contains(c))
        .map_or(c, |&(_, base)| base)
}

/// Every site, sorted by name. A linked site wins over a parked folder of the
/// same name; among parked directories the first one listed wins.
pub fn all(config: &Config) -> Vec<Site> {
    let mut sites: Vec<Site> = config
        .sites
        .iter()
        .map(|s| Site {
            name: s.name.clone(),
            path: s.path.clone(),
            kind: Kind::Linked,
            php: s.php.clone(),
        })
        .collect();
    for dir in &config.parked {
        for (name, path) in parked_folders(dir) {
            if !sites.iter().any(|s| s.name == name) {
                sites.push(Site {
                    name,
                    path,
                    kind: Kind::Parked,
                    php: None,
                });
            }
        }
    }
    sites.sort_by(|a, b| a.name.cmp(&b.name));
    sites
}

/// Visible sub-folders of a parked directory with their site names.
fn parked_folders(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let folder = e.file_name().into_string().ok()?;
            if folder.starts_with('.') {
                return None;
            }
            normalize_name(&folder).map(|n| (n, e.path()))
        })
        .collect()
}

/// The site a request `Host` (port already stripped, lowercase) refers to.
pub fn find<'a>(sites: &'a [Site], host: &str, tld: &str) -> Option<&'a Site> {
    let name = host.strip_suffix(tld)?.strip_suffix('.')?;
    sites.iter().find(|s| s.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    #[test]
    fn names() {
        assert_eq!(normalize_name("My App").as_deref(), Some("my-app"));
        assert_eq!(normalize_name("shop_v2..beta").as_deref(), Some("shop-v2-beta"));
        assert_eq!(normalize_name("Dự án").as_deref(), Some("du-an"));
        assert_eq!(normalize_name("Đồ Án Tốt Nghiệp").as_deref(), Some("do-an-tot-nghiep"));
        assert_eq!(normalize_name("___"), None);
    }

    #[test]
    fn linked_wins_and_docroot_detection() {
        let root = std::env::temp_dir().join(format!("phpev-sites-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let parked = root.join("code");
        std::fs::create_dir_all(parked.join(r"Blog\public")).unwrap();
        std::fs::write(parked.join(r"Blog\public\index.php"), "").unwrap();
        std::fs::create_dir_all(parked.join("plain")).unwrap();
        std::fs::create_dir_all(parked.join(".hidden")).unwrap();
        let elsewhere = root.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();

        let cfg = Config {
            parked: vec![parked.clone()],
            sites: vec![config::Site {
                name: "plain".into(),
                path: elsewhere.clone(),
                php: Some("8.3".into()),
                secure: false,
                defender_excluded: false,
            }],
            ..Config::default()
        };
        let sites = all(&cfg);
        let names: Vec<&str> = sites.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["blog", "plain"]);

        let blog = find(&sites, "blog.localhost", "localhost").unwrap();
        assert_eq!(blog.docroot(), parked.join(r"Blog\public"));
        let plain = find(&sites, "plain.localhost", "localhost").unwrap();
        assert_eq!(
            (plain.kind, plain.path.clone(), plain.docroot()),
            (Kind::Linked, elsewhere.clone(), elsewhere)
        );
        assert!(find(&sites, "blog.test", "localhost").is_none());
        assert!(find(&sites, "localhost", "localhost").is_none());

        std::fs::remove_dir_all(&root).unwrap();
    }
}
