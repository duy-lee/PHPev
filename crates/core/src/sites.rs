//! Site table: the linked sites of `config.toml`.

use std::path::PathBuf;

use crate::config::Config;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Site {
    /// DNS label, e.g. `my-app` for `my-app.localhost`.
    pub name: String,
    pub path: PathBuf,
    /// PHP version pinned in config; `.php-version` still wins.
    pub php: Option<String>,
    /// Composer version picked for the site.
    pub composer: Option<String>,
    /// HTTPS enabled with `phpev secure`.
    pub secure: bool,
    /// Databases added by hand in the dashboard, lower case.
    pub databases: Vec<String>,
    /// Databases its `.env` names that were taken off the Database tab, lower case.
    pub hidden_databases: Vec<String>,
    /// Folder inside the project that is served, relative (`public`, `.` for the project folder
    /// itself); `None` picks it by itself.
    pub docroot: Option<String>,
}

impl Site {
    /// The folder the web server serves: the one picked for the site (while it
    /// exists), else [`Site::auto_docroot`].
    pub fn docroot(&self) -> PathBuf {
        self.docroot
            .as_deref()
            .map(|rel| self.path.join(rel))
            .filter(|dir| dir.is_dir())
            .unwrap_or_else(|| self.auto_docroot())
    }

    /// `public/` when it holds a front controller (Laravel, Symfony, Slim…),
    /// otherwise the project folder itself (plain PHP).
    pub fn auto_docroot(&self) -> PathBuf {
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

/// A document root typed or picked by the user, as `config.toml` keeps it: relative to the
/// project, `/` between folders, `.` for the project itself. `None` when it is empty,
/// absolute, on another drive or climbs out of the project with `..`.
pub fn normalize_docroot(rel: &str) -> Option<String> {
    let rel = rel.trim();
    if rel.is_empty() || rel.starts_with(['/', '\\']) {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in rel.split(['/', '\\']).map(str::trim) {
        match part {
            "" | "." => {}
            ".." => return None,
            p if p.contains(':') => return None,
            p => parts.push(p),
        }
    }
    Some(if parts.is_empty() { ".".into() } else { parts.join("/") })
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

/// Every site, sorted by name.
pub fn all(config: &Config) -> Vec<Site> {
    let mut sites: Vec<Site> = config
        .sites
        .iter()
        .map(|s| Site {
            name: s.name.clone(),
            path: s.path.clone(),
            php: s.php.clone(),
            composer: s.composer.clone(),
            secure: s.secure,
            databases: s.databases.clone(),
            hidden_databases: s.hidden_databases.clone(),
            docroot: s.docroot.clone(),
        })
        .collect();
    sites.sort_by(|a, b| a.name.cmp(&b.name));
    sites
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
    fn docroot_is_picked_or_detected() {
        let root = std::env::temp_dir().join(format!("phpev-sites-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let laravel = root.join("Blog");
        std::fs::create_dir_all(laravel.join("public")).unwrap();
        std::fs::write(laravel.join("public").join("index.php"), "").unwrap();
        let plain = root.join("plain");
        std::fs::create_dir_all(plain.join("web").join("dist")).unwrap();
        let entry = |name: &str, path: &PathBuf, docroot: Option<&str>| config::Site {
            name: name.into(),
            path: path.clone(),
            php: None,
            composer: None,
            secure: false,
            databases: Vec::new(),
            hidden_databases: Vec::new(),
            docroot: docroot.map(str::to_owned),
            defender_excluded: false,
        };
        let cfg = Config {
            sites: vec![
                entry("plain", &plain, None),
                entry("blog", &laravel, None),
                entry("dist", &plain, Some("web/dist")),
                entry("gone", &plain, Some("removed")),
                entry("whole", &laravel, Some(".")),
            ],
            ..Config::default()
        };
        let sites = all(&cfg);
        let names: Vec<&str> = sites.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["blog", "dist", "gone", "plain", "whole"]);

        let docroot = |name: &str| {
            find(&sites, &format!("{name}.localhost"), "localhost")
                .unwrap()
                .docroot()
        };
        assert_eq!(
            docroot("blog"),
            laravel.join("public"),
            "public/ with a front controller"
        );
        assert_eq!(docroot("plain"), plain);
        assert_eq!(docroot("dist"), plain.join("web/dist"));
        assert_eq!(
            docroot("gone"),
            plain,
            "a folder that is gone falls back to the automatic one"
        );
        assert_eq!(
            docroot("whole"),
            laravel.join("."),
            "`.` serves the project folder though public/ exists"
        );
        assert!(find(&sites, "blog.test", "localhost").is_none());
        assert!(find(&sites, "localhost", "localhost").is_none());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn docroot_names_stay_inside_the_project() {
        assert_eq!(normalize_docroot(" public ").as_deref(), Some("public"));
        assert_eq!(normalize_docroot(r"web\public/").as_deref(), Some("web/public"));
        assert_eq!(normalize_docroot("./web//dist").as_deref(), Some("web/dist"));
        assert_eq!(normalize_docroot(".").as_deref(), Some("."));
        for bad in ["", "  ", "..", "web/../..", r"C:\x", "/etc", r"\\share"] {
            assert_eq!(normalize_docroot(bad), None, "{bad}");
        }
    }
}
