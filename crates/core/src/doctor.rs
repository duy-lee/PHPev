//! Environment health checks shared by `phpev setup` and `phpev doctor`.

use std::path::PathBuf;

use crate::config::Config;
use crate::env_path::{self, Scope};
use crate::layout::{ascii_short_path, HomeSource, Layout, PathRisk, FALLBACK_HOME};
use crate::ports;
use crate::registry::{self, Hive};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Ok,
    Warn,
    Fail,
}

#[derive(Clone, Debug)]
pub struct Check {
    pub status: Status,
    pub title: String,
    /// Extra lines explaining the finding.
    pub details: Vec<String>,
    /// What the user can do about it.
    pub hint: Option<String>,
}

impl Check {
    fn new(status: Status, title: impl Into<String>) -> Self {
        Self {
            status,
            title: title.into(),
            details: Vec::new(),
            hint: None,
        }
    }

    fn detail(mut self, line: impl Into<String>) -> Self {
        self.details.push(line.into());
        self
    }

    fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// Facts about PATH ordering that `doctor --fix-path` acts on.
#[derive(Debug, Default)]
pub struct PathReport {
    /// A PHP/Composer found on the *system* PATH would win over the shims.
    pub shadowed_by_system: bool,
}

pub fn run(layout: &Layout, config: &Config) -> (Vec<Check>, PathReport) {
    let mut checks = vec![home_check(layout), layout_check(layout)];
    let (path_checks, report) = path_checks(layout);
    checks.extend(path_checks);
    checks.push(vc_runtime_check());
    checks.push(defender_check(config));
    checks.push(cacert_check(layout));
    for (name, port) in [
        ("HTTP", config.ports.http),
        ("HTTPS", config.ports.https),
        ("MariaDB", config.ports.mariadb),
    ] {
        checks.push(port_check(name, port));
    }
    (checks, report)
}

fn home_check(layout: &Layout) -> Check {
    let root = layout.root();
    let risk = PathRisk::of(root);
    let source = match layout.source() {
        HomeSource::ProcessEnv => "PHPEV_HOME",
        HomeSource::UserEnv => "PHPEV_HOME (user)",
        HomeSource::Default => "default",
    };
    if risk.is_clean() {
        return Check::new(Status::Ok, format!("Home: {} ({source})", root.display()));
    }
    let hint = format!("phpev setup --home {FALLBACK_HOME}");
    if !risk.non_ascii {
        return Check::new(Status::Warn, format!("Home path contains spaces: {}", root.display())).hint(hint);
    }
    if !root.is_dir() {
        // The 8.3 short name only exists once the directory does.
        return Check::new(Status::Warn, format!("Home path is not ASCII: {}", root.display()))
            .detail("PHP cannot load php.ini from a non-ASCII path")
            .hint(hint);
    }
    // PHP cannot read php.ini from a non-ASCII path; the 8.3 short name is the only workaround.
    match ascii_short_path(root) {
        Some(short) => Check::new(Status::Warn, format!("Home path is not ASCII: {}", root.display()))
            .detail(format!("PHP will be given the 8.3 short path {}", short.display()))
            .hint(hint),
        None => Check::new(Status::Fail, format!("Home path is not ASCII: {}", root.display()))
            .detail("PHP cannot load php.ini from it and no ASCII 8.3 short name is available")
            .hint(hint),
    }
}

fn layout_check(layout: &Layout) -> Check {
    let missing: Vec<PathBuf> = [
        layout.shims(),
        layout.php_dir(),
        layout.mariadb_dir(),
        layout.logs_dir(),
    ]
    .into_iter()
    .filter(|d| !d.is_dir())
    .collect();
    if missing.is_empty() && layout.config_file().is_file() {
        Check::new(Status::Ok, "Directory layout and config.toml present")
    } else {
        Check::new(Status::Fail, "PHPev is not set up yet").hint("phpev setup")
    }
}

fn path_checks(layout: &Layout) -> (Vec<Check>, PathReport) {
    let shims = layout.shims();
    let shims_str = shims.to_string_lossy();
    let mut checks = Vec::new();
    let mut report = PathReport::default();

    let user_raw = env_path::get_raw(Scope::User, "Path")
        .ok()
        .flatten()
        .unwrap_or_default();
    checks.push(match env_path::position(&user_raw, &shims_str) {
        Some(0) => Check::new(Status::Ok, "Shims are first on the user PATH"),
        Some(_) => Check::new(Status::Warn, "Shims are on the user PATH but not first").hint("phpev setup"),
        None => Check::new(Status::Fail, "Shims are not on the user PATH").hint("phpev setup"),
    });

    let effective = match env_path::effective_path() {
        Ok(p) => p,
        Err(e) => {
            checks.push(Check::new(
                Status::Warn,
                format!("Could not read PATH from the registry: {e}"),
            ));
            return (checks, report);
        }
    };

    for (tool, names) in [
        ("php", &["php.exe"][..]),
        ("composer", &["composer.exe", "composer.bat", "composer.cmd"][..]),
    ] {
        // Entries that would run instead of our shim: everything resolved before it.
        let others: Vec<(Scope, PathBuf)> = env_path::which_all(&effective, names)
            .into_iter()
            .take_while(|(_, p)| !env_path::is_in_dir(p, &shims))
            .collect();
        let shim_exists = names.iter().any(|n| shims.join(n).is_file());

        if others.is_empty() {
            checks.push(Check::new(
                Status::Ok,
                format!("`{tool}` in new terminals will resolve to the PHPev shim"),
            ));
            continue;
        }

        let in_system = others.iter().any(|(s, _)| *s == Scope::System);
        report.shadowed_by_system |= in_system;
        // Before the shims are installed (M3) a foreign tool is only a heads-up.
        let status = if shim_exists { Status::Fail } else { Status::Warn };
        let mut c = Check::new(status, format!("Another `{tool}` comes before the PHPev shim on PATH"));
        for (scope, p) in &others {
            let label = if *scope == Scope::System {
                "system PATH"
            } else {
                "user PATH"
            };
            c = c.detail(format!("{} ({label})", p.display()));
        }
        c = if in_system {
            c.hint("phpev doctor --fix-path   (puts the shims first on the system PATH; needs admin)")
        } else {
            c.hint("phpev setup   (moves the shims to the front of the user PATH)")
        };
        checks.push(c);
    }
    (checks, report)
}

fn vc_runtime_check() -> Check {
    // This process is always 64-bit, so this reads the 64-bit registry view.
    const KEY: &str = r"SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64";
    let installed = registry::read_dword(Hive::LocalMachine, KEY, "Installed")
        .ok()
        .flatten();
    let version = registry::read_string(Hive::LocalMachine, KEY, "Version").ok().flatten();
    match installed {
        Some(1) => Check::new(
            Status::Ok,
            format!(
                "VC++ 2015-2022 x64 runtime installed{}",
                version.map(|v| format!(" ({v})")).unwrap_or_default()
            ),
        ),
        _ => Check::new(Status::Warn, "VC++ 2015-2022 x64 runtime not found (needed by PHP)")
            .hint("phpev will install it together with the first PHP version"),
    }
}

fn defender_check(config: &Config) -> Check {
    if config.defender_excluded {
        Check::new(Status::Ok, "Microsoft Defender exclusion added for the PHPev home")
    } else {
        Check::new(
            Status::Warn,
            "No Microsoft Defender exclusion for the PHPev home (every php/composer call is scanned)",
        )
        .detail("on-access scanning adds to the start-up time of every PHP process")
        .hint("phpev doctor --defender-exclude   (needs admin; skip if you use another antivirus)")
    }
}

fn cacert_check(layout: &Layout) -> Check {
    if layout.cacert().is_file() {
        Check::new(Status::Ok, "CA bundle present (cacert.pem)")
    } else {
        Check::new(
            Status::Warn,
            "CA bundle (cacert.pem) missing — Composer/HTTPS from PHP will fail",
        )
        .hint("downloaded automatically with the first PHP install")
    }
}

fn port_check(name: &str, port: u16) -> Check {
    match ports::listeners_on(port) {
        Ok(found) if found.is_empty() => Check::new(Status::Ok, format!("Port {port} ({name}) is free")),
        Ok(found) => {
            let mut c = Check::new(Status::Warn, format!("Port {port} ({name}) is in use"));
            let mut seen = Vec::new();
            for l in found {
                if seen.contains(&l.pid) {
                    continue;
                }
                seen.push(l.pid);
                let who = ports::known_culprit(&l.process)
                    .map(|k| format!(" — {k}"))
                    .unwrap_or_default();
                c = c.detail(format!("{} (pid {}){who}", l.process, l.pid));
            }
            c.hint("stop that program, or change [ports] in config.toml")
        }
        Err(e) => Check::new(Status::Warn, format!("Could not inspect port {port}: {e}")),
    }
}
