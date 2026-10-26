//! What a project folder is: its framework, its database settings and whether
//! it is ready to run. Shared by `phpev import` and the dashboard's site page.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::envfile;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Framework {
    /// `Laravel`, `Symfony`, `WordPress`, `PHP` (anything else with PHP files) or `Static`.
    pub name: String,
    /// From `composer.lock` or WordPress's `version.php`, when known.
    pub version: Option<String>,
}

pub fn read_json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Installed version of a Composer package, from `composer.lock`.
pub fn package_version(lock: &serde_json::Value, name: &str) -> Option<String> {
    ["packages", "packages-dev"].iter().find_map(|key| {
        lock.get(*key)?.as_array()?.iter().find_map(|p| {
            (p.get("name")?.as_str()? == name)
                .then(|| p.get("version")?.as_str().map(|v| v.trim_start_matches('v').to_owned()))?
        })
    })
}

fn requires(composer_json: Option<&serde_json::Value>, package: &str) -> bool {
    composer_json.is_some_and(|c| {
        ["require", "require-dev"]
            .iter()
            .any(|k| c.get(*k).and_then(|r| r.get(package)).is_some())
    })
}

pub fn is_laravel(dir: &Path, composer_json: Option<&serde_json::Value>) -> bool {
    dir.join("artisan").is_file() && requires(composer_json, "laravel/framework")
}

/// Detects the framework of the project in `dir` from its files only (fast,
/// nothing is run).
pub fn detect(dir: &Path) -> Framework {
    let composer_json = read_json(&dir.join("composer.json"));
    let lock = || read_json(&dir.join("composer.lock"));
    let named = |name: &str, package: &str| Framework {
        name: name.into(),
        version: lock().and_then(|l| package_version(&l, package)),
    };
    if is_laravel(dir, composer_json.as_ref()) {
        let mut found = named("Laravel", "laravel/framework");
        // Older projects often keep composer.lock out of the repository.
        if found.version.is_none() {
            found.version = std::fs::read_to_string(
                dir.join(r"vendor\laravel\framework\src\Illuminate\Foundation\Application.php"),
            )
            .ok()
            .and_then(|t| php_string_after(&t, "const VERSION"));
        }
        return found;
    }
    if requires(composer_json.as_ref(), "symfony/framework-bundle") {
        return named("Symfony", "symfony/framework-bundle");
    }
    let wp_version = dir.join(r"wp-includes\version.php");
    if dir.join("wp-config.php").is_file() || wp_version.is_file() {
        return Framework {
            name: "WordPress".into(),
            version: std::fs::read_to_string(wp_version)
                .ok()
                .and_then(|t| php_string_after(&t, "$wp_version")),
        };
    }
    let has_php = composer_json.is_some()
        || std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .any(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("php")));
    Framework {
        name: if has_php { "PHP" } else { "Static" }.into(),
        version: None,
    }
}

/// The first quoted string after `marker` (e.g. `$wp_version = '6.6.2';`).
fn php_string_after(text: &str, marker: &str) -> Option<String> {
    let rest = &text[text.find(marker)? + marker.len()..];
    let quote_at = rest.find(['\'', '"'])?;
    let quote = rest[quote_at..].chars().next()?;
    let value = &rest[quote_at + 1..];
    Some(value[..value.find(quote)?].to_owned())
}

/// PHP extensions a project needs (`ext-*` in composer.json and composer.lock), lower case, sorted.
pub fn required_extensions(composer_json: Option<&serde_json::Value>, lock: Option<&serde_json::Value>) -> Vec<String> {
    let mut wanted: Vec<String> = Vec::new();
    let mut collect = |req: Option<&serde_json::Value>| {
        for key in req.and_then(|r| r.as_object()).into_iter().flat_map(|o| o.keys()) {
            if let Some(ext) = key.strip_prefix("ext-") {
                let ext = ext.to_ascii_lowercase();
                if !wanted.contains(&ext) {
                    wanted.push(ext);
                }
            }
        }
    };
    collect(composer_json.and_then(|c| c.get("require")));
    for key in ["packages", "packages-dev"] {
        for p in lock.and_then(|l| l.get(key)?.as_array()).into_iter().flatten() {
            collect(p.get("require"));
        }
    }
    wanted.sort();
    wanted
}

/// `php -m` names compared with composer's `ext-*` names.
pub fn normalize_module(m: &str) -> String {
    let m = m.to_ascii_lowercase().replace(['-', ' '], "_");
    match m.as_str() {
        "zend_opcache" => "opcache".into(),
        _ => m,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbSettings {
    /// The `.env` prefix of this connection: empty for `DB_*`, `MAIN` for
    /// `MAIN_DB_*`; `wordpress` for wp-config.php.
    pub name: String,
    /// `DB_CONNECTION`: a driver (`mysql`, `sqlite`, …) or a connection name
    /// from config/database.php (`main`).
    pub connection: String,
    pub host: String,
    pub port: Option<u16>,
    /// Empty when the project picks it at run time.
    pub database: String,
    pub username: String,
    pub has_password: bool,
}

impl DbSettings {
    /// True when the project talks to a MySQL-compatible server on this
    /// computer. A connection name that is not a known other driver is
    /// taken for MySQL, as Laravel projects define it in config/database.php.
    pub fn is_local_mysql(&self, mariadb_port: u16) -> bool {
        !matches!(self.connection.as_str(), "sqlite" | "pgsql" | "sqlsrv" | "mongodb")
            && matches!(self.host.as_str(), "127.0.0.1" | "localhost" | "::1")
            && self.port.unwrap_or(3306) == mariadb_port
    }
}

/// Database connections from `.env` (`DB_*` and prefixed ones such as
/// `MAIN_DB_*`, in file order) or from WordPress's `wp-config.php`.
pub fn databases(dir: &Path) -> Vec<DbSettings> {
    if let Ok(env) = std::fs::read_to_string(dir.join(".env")) {
        let mut prefixes: Vec<&str> = Vec::new();
        for key in envfile::keys(&env) {
            let Some(prefix) = ["DB_HOST", "DB_DATABASE", "DB_CONNECTION"]
                .iter()
                .find_map(|k| key.strip_suffix(k))
            else {
                continue;
            };
            if (prefix.is_empty() || prefix.ends_with('_')) && !prefixes.contains(&prefix) {
                prefixes.push(prefix);
            }
        }
        return prefixes
            .into_iter()
            .map(|p| {
                let get = |k: &str| envfile::get(&env, &format!("{p}DB_{k}"));
                DbSettings {
                    name: p.trim_end_matches('_').to_owned(),
                    connection: get("CONNECTION").unwrap_or_else(|| "mysql".into()),
                    host: get("HOST").unwrap_or_else(|| "127.0.0.1".into()),
                    port: get("PORT").and_then(|v| v.parse().ok()),
                    database: get("DATABASE").unwrap_or_default(),
                    username: get("USERNAME").unwrap_or_default(),
                    has_password: get("PASSWORD").is_some_and(|v| !v.is_empty()),
                }
            })
            .collect();
    }
    let Ok(wp) = std::fs::read_to_string(dir.join("wp-config.php")) else {
        return Vec::new();
    };
    let define = |key: &str| php_string_after(&wp, &format!("'{key}',"));
    let host = define("DB_HOST").unwrap_or_else(|| "localhost".into());
    let (host, port) = match host.split_once(':') {
        Some((h, p)) => (h.to_owned(), p.parse().ok()),
        None => (host, None),
    };
    vec![DbSettings {
        name: "wordpress".into(),
        connection: "mysql".into(),
        host,
        port,
        database: define("DB_NAME").unwrap_or_default(),
        username: define("DB_USER").unwrap_or_default(),
        has_password: define("DB_PASSWORD").is_some_and(|p| !p.is_empty()),
    }]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    /// Stable id (`dependencies`, `env`, `app_key`, `extensions`, `database`).
    pub id: String,
    pub status: CheckStatus,
    pub title: String,
    /// What to do about it, when not ok.
    pub hint: Option<String>,
}

fn check(id: &str, status: CheckStatus, title: String, hint: Option<&str>) -> Check {
    Check {
        id: id.into(),
        status,
        title,
        hint: hint.map(str::to_owned),
    }
}

/// What the dashboard needs to know to judge a project.
pub struct Facts<'a> {
    /// `php -m` of the site's PHP, `None` if it could not run.
    pub modules: Option<&'a [String]>,
    /// Databases on PHPev's MariaDB; `None` when it is not running (or could not be asked).
    pub existing_databases: Option<&'a [String]>,
}

/// Readiness checks of the project in `dir`, problems first.
pub fn checks(dir: &Path, facts: &Facts, mariadb_port: u16) -> Vec<Check> {
    use CheckStatus::{Fail, Ok, Warn};
    let composer_json = read_json(&dir.join("composer.json"));
    let lock = read_json(&dir.join("composer.lock"));
    let mut out = Vec::new();

    if composer_json.is_some() {
        out.push(if dir.join(r"vendor\autoload.php").is_file() {
            check("dependencies", Ok, "Dependencies installed".into(), None)
        } else {
            check(
                "dependencies",
                Fail,
                "Dependencies are not installed".into(),
                Some("Run `composer install` in the project folder"),
            )
        });
    }

    if is_laravel(dir, composer_json.as_ref()) {
        match std::fs::read_to_string(dir.join(".env")) {
            Err(_) => out.push(check(
                "env",
                Fail,
                ".env is missing".into(),
                Some("Copy .env.example to .env, then run `php artisan key:generate`"),
            )),
            Result::Ok(env) => {
                out.push(check("env", Ok, ".env present".into(), None));
                out.push(if envfile::get(&env, "APP_KEY").is_some_and(|k| !k.is_empty()) {
                    check("app_key", Ok, "Application key set".into(), None)
                } else {
                    check(
                        "app_key",
                        Fail,
                        "APP_KEY is not set".into(),
                        Some("Run `php artisan key:generate`"),
                    )
                });
            }
        }
    }

    let wanted = required_extensions(composer_json.as_ref(), lock.as_ref());
    if let (false, Some(modules)) = (wanted.is_empty(), facts.modules) {
        let loaded: Vec<String> = modules.iter().map(|m| normalize_module(m)).collect();
        let missing: Vec<&String> = wanted
            .iter()
            .filter(|e| !loaded.contains(&normalize_module(e)))
            .collect();
        out.push(if missing.is_empty() {
            check("extensions", Ok, "Required extensions loaded".into(), None)
        } else {
            let names: Vec<&str> = missing.iter().map(|s| s.as_str()).collect();
            check(
                "extensions",
                Warn,
                format!("Missing extensions: {}", names.join(", ")),
                Some("Turn them on in PHP › Extensions"),
            )
        });
    }

    let local: Vec<DbSettings> = databases(dir)
        .into_iter()
        .filter(|d| d.is_local_mysql(mariadb_port) && !d.database.is_empty())
        .collect();
    match facts.existing_databases {
        None if !local.is_empty() => out.push(check(
            "database",
            Warn,
            "MariaDB is not running".into(),
            Some("Start it on the Services page"),
        )),
        None => {}
        Some(existing) => {
            for db in &local {
                let id = if db.name.is_empty() {
                    "database".to_owned()
                } else {
                    format!("database:{}", db.name)
                };
                out.push(if existing.iter().any(|e| e.eq_ignore_ascii_case(&db.database)) {
                    check(&id, Ok, format!("Database `{}` exists", db.database), None)
                } else {
                    check(
                        &id,
                        Fail,
                        format!("Database `{}` does not exist", db.database),
                        Some("Create it, or import a dump on the Database tab"),
                    )
                });
            }
        }
    }

    out.sort_by_key(|c| match c.status {
        Fail => 0,
        Warn => 1,
        Ok => 2,
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("phpev-project-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, text) in files {
            let file = dir.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const LARAVEL_JSON: &str = r#"{"require":{"php":"^8.2","laravel/framework":"^11.0","ext-intl":"*"}}"#;
    const LARAVEL_LOCK: &str =
        r#"{"packages":[{"name":"laravel/framework","version":"v11.31.0","require":{"ext-mbstring":"*"}}]}"#;

    #[test]
    fn frameworks_are_detected_from_files() {
        let laravel = project(
            "laravel",
            &[
                ("artisan", ""),
                ("composer.json", LARAVEL_JSON),
                ("composer.lock", LARAVEL_LOCK),
            ],
        );
        assert_eq!(
            detect(&laravel),
            Framework {
                name: "Laravel".into(),
                version: Some("11.31.0".into())
            }
        );
        let symfony = project(
            "symfony",
            &[("composer.json", r#"{"require":{"symfony/framework-bundle":"7.1.*"}}"#)],
        );
        assert_eq!(detect(&symfony).name, "Symfony");
        let wp = project(
            "wp",
            &[
                ("wp-config.php", ""),
                ("wp-includes/version.php", "<?php\n$wp_version = '6.6.2';\n"),
            ],
        );
        assert_eq!(
            detect(&wp),
            Framework {
                name: "WordPress".into(),
                version: Some("6.6.2".into())
            }
        );
        assert_eq!(detect(&project("plain", &[("index.php", "")])).name, "PHP");
        assert_eq!(detect(&project("static", &[("index.html", "")])).name, "Static");
        for d in [laravel, symfony, wp] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn laravel_version_without_a_lock_file_comes_from_vendor() {
        let dir = project(
            "nolock",
            &[
                ("artisan", ""),
                ("composer.json", LARAVEL_JSON),
                (
                    "vendor/laravel/framework/src/Illuminate/Foundation/Application.php",
                    "<?php\nclass Application {\n    const VERSION = '5.5.50';\n}\n",
                ),
            ],
        );
        assert_eq!(detect(&dir).version.as_deref(), Some("5.5.50"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn database_settings_from_env_and_wp_config() {
        let env = project(
            "env",
            &[(
                ".env",
                "DB_CONNECTION=mysql
DB_HOST=127.0.0.1
DB_PORT=3306
DB_DATABASE=shop
DB_USERNAME=root
DB_PASSWORD=
",
            )],
        );
        let dbs = databases(&env);
        assert_eq!(dbs.len(), 1);
        assert_eq!(
            (
                dbs[0].name.as_str(),
                dbs[0].database.as_str(),
                dbs[0].port,
                dbs[0].has_password
            ),
            ("", "shop", Some(3306), false)
        );
        assert!(dbs[0].is_local_mysql(3306));
        assert!(!dbs[0].is_local_mysql(3307));
        let wp = project(
            "wpdb",
            &[(
                "wp-config.php",
                "define( 'DB_NAME', 'blog' );
define( 'DB_USER', 'root' );
define( 'DB_HOST', 'localhost:3306' );",
            )],
        );
        let db = &databases(&wp)[0];
        assert_eq!(
            (db.database.as_str(), db.host.as_str(), db.port),
            ("blog", "localhost", Some(3306))
        );
        let _ = std::fs::remove_dir_all(env);
        let _ = std::fs::remove_dir_all(wp);
    }

    #[test]
    fn prefixed_connections_with_inline_comments() {
        let dir = project(
            "prefixed",
            &[(
                ".env",
                "APP_KEY=x
MAIN_DB_CONNECTION=main
MAIN_DB_HOST=127.0.0.1
MAIN_DB_PORT=3306
MAIN_DB_DATABASE=gps_local_main # gps_master
                 MAIN_DB_USERNAME=root
SUB_DB_HOST=127.0.0.1
SUB_DB_PORT=3306
SUB_DB_USERNAME=root
REDIS_HOST=127.0.0.1
",
            )],
        );
        let dbs = databases(&dir);
        let summary: Vec<(&str, &str, &str)> = dbs
            .iter()
            .map(|d| (d.name.as_str(), d.connection.as_str(), d.database.as_str()))
            .collect();
        assert_eq!(summary, [("MAIN", "main", "gps_local_main"), ("SUB", "mysql", "")]);
        assert!(dbs[0].is_local_mysql(3306), "a connection name is taken for MySQL");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn laravel_checks_report_what_is_missing_first() {
        let dir = project(
            "checks",
            &[
                ("artisan", ""),
                ("composer.json", LARAVEL_JSON),
                ("composer.lock", LARAVEL_LOCK),
                (".env", "APP_KEY=\nDB_CONNECTION=mysql\nDB_DATABASE=shop\n"),
            ],
        );
        let modules = vec!["mbstring".to_owned(), "Zend OPcache".to_owned()];
        let existing = vec!["shop".to_owned()];
        let facts = Facts {
            modules: Some(&modules),
            existing_databases: Some(&existing),
        };
        let got = checks(&dir, &facts, 3306);
        let ids: Vec<(&str, CheckStatus)> = got.iter().map(|c| (c.id.as_str(), c.status)).collect();
        assert_eq!(
            ids,
            [
                ("dependencies", CheckStatus::Fail),
                ("app_key", CheckStatus::Fail),
                ("extensions", CheckStatus::Warn),
                ("env", CheckStatus::Ok),
                ("database", CheckStatus::Ok),
            ]
        );
        assert_eq!(got[2].title, "Missing extensions: intl");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn module_names_match_composer_ext_names() {
        assert_eq!(normalize_module("Zend OPcache"), "opcache");
        assert_eq!(normalize_module("pdo_mysql"), normalize_module("PDO_MYSQL"));
        assert_eq!(normalize_module("SimpleXML"), "simplexml");
    }

    #[test]
    fn lock_versions() {
        let lock: serde_json::Value =
            serde_json::from_str(r#"{"packages":[{"name":"laravel/framework","version":"v12.3.1"}]}"#).unwrap();
        assert_eq!(package_version(&lock, "laravel/framework").as_deref(), Some("12.3.1"));
    }
}
