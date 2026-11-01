//! `phpev php …`, `phpev db …` and `phpev composer …`.

use std::io::{self, IsTerminal};
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use phpev_core::config::Config;
use phpev_core::ipc::{self, Request};
use phpev_core::layout::Layout;
use phpev_core::net::Http;
use phpev_core::{clock, composer, mariadb, ops, php, shim, version};

use crate::progress::{mib, Terminal};

/// Loads the layout, creating its directories if `phpev setup` has not run yet.
fn layout() -> io::Result<Layout> {
    let layout = Layout::resolve()?;
    layout.create_dirs()?;
    Ok(layout)
}

// ---------------------------------------------------------------- PHP

pub fn php_list(available: bool) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    let config = Config::load(&layout.config_file())?;
    let installed = php::installed(&layout);
    let default = config
        .php
        .as_deref()
        .and_then(|w| version::best_match(w, installed.iter().map(String::as_str)));

    println!("Installed:");
    if installed.is_empty() {
        println!("  (none) — try: phpev php install 8.4");
    }
    for v in &installed {
        let mark = if Some(v.as_str()) == default { "  (default)" } else { "" };
        println!("  {v}{mark}");
    }

    if available {
        println!("\nAvailable (windows.php.net, NTS x64):");
        for r in php::available(&Http::new()?)? {
            let mut mark = String::new();
            if r.archived {
                mark.push_str("  end of life");
            }
            if installed.contains(&r.version) {
                mark.push_str("  installed");
            }
            println!("  {:<8} {:<8} {}{mark}", r.line, r.version, r.toolset);
        }
    }
    Ok(ExitCode::SUCCESS)
}

pub fn php_install(wanted: &str) -> io::Result<ExitCode> {
    let started = Instant::now();
    let layout = layout()?;
    let version = ops::install_php(&Http::new()?, &layout, wanted, &mut Terminal::default())?;
    println!(
        "Done in {:.1}s → {}",
        started.elapsed().as_secs_f64(),
        php::version_dir(&layout, &version).display()
    );
    Ok(ExitCode::SUCCESS)
}

pub fn php_add(dir: &Path) -> io::Result<ExitCode> {
    let layout = layout()?;
    let version = ops::add_php(&Http::new()?, &layout, dir, &mut Terminal::default())?;
    println!(
        "Added PHP {version} → {}",
        php::version_dir(&layout, &version).display()
    );
    Ok(ExitCode::SUCCESS)
}

pub fn php_remove(version: &str, yes: bool) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    ops::check_remove_php(&layout, version)?;
    let pinned = ops::sites_pinned_to(&layout, version)?;
    if !pinned.is_empty() {
        println!(
            "These sites are pinned to PHP {version} and will show an error until you pick another PHP for them: {}",
            pinned.join(", ")
        );
        if !yes && !confirm(&format!("Remove PHP {version} anyway? [y/N] "))? {
            println!("Nothing was changed.");
            return Ok(ExitCode::SUCCESS);
        }
    }
    if ipc::is_running() {
        // Running php-cgi workers keep the files in use.
        crate::daemon::request(&Request::PhpRestart {
            version: version.to_owned(),
        })?;
    }
    ops::remove_php(&layout, version, &mut Terminal::default())?;
    Ok(ExitCode::SUCCESS)
}

/// Asks a yes/no question; refuses without a terminal to ask in.
fn confirm(prompt: &str) -> io::Result<bool> {
    if !io::stdin().is_terminal() {
        return Err(io::Error::other("no terminal to confirm in; re-run with --yes"));
    }
    crate::uninstall::ask(prompt, &["y", "yes"])
}

pub fn php_use(wanted: &str) -> io::Result<ExitCode> {
    let found = ops::set_default_php(&Layout::resolve()?, wanted)?;
    println!("Default PHP is now {wanted} (currently {found}).");
    Ok(ExitCode::SUCCESS)
}

pub fn php_ini(wanted: &str) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    let installed = php::installed(&layout);
    let Some(found) = version::best_match(wanted, installed.iter().map(String::as_str)) else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("PHP {wanted} is not installed"),
        ));
    };
    if php::write_php_ini(&layout, found)? {
        println!("Generated php.ini for {found}.");
    }
    println!("{}", php::version_dir(&layout, found).join("php.ini").display());
    Ok(ExitCode::SUCCESS)
}

pub fn php_which() -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    let cwd = std::env::current_dir()?;
    match shim::resolve(layout.root(), &cwd) {
        Ok(r) => {
            println!("PHP {} ({})", r.version, r.dir.join("php.exe").display());
            println!("  wanted {} from {}", r.wanted, r.source);
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("{e}");
            Ok(ExitCode::FAILURE)
        }
    }
}

// ---------------------------------------------------------------- MariaDB

pub fn db_list(available: bool) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    let config = Config::load(&layout.config_file())?;
    let installed = mariadb::installed(&layout);

    println!("Installed:");
    if installed.is_empty() {
        println!("  (none) — try: phpev db install 11.4");
    }
    for v in &installed {
        let active = config.mariadb.as_deref() == Some(mariadb::major_of(v).as_str());
        println!("  {v}{}", if active { "  (active)" } else { "" });
    }

    if available {
        println!("\nAvailable (stable):");
        let today = clock::today();
        for m in mariadb::majors(&Http::new()?)? {
            let lts = if m.is_lts() { "LTS" } else { "rolling" };
            let eol = match m.release_eol_date.as_deref() {
                Some(d) if m.is_eol(&today) => format!("  EOL {d} — no longer supported"),
                Some(d) => format!("  EOL {d}"),
                None => String::new(),
            };
            println!("  {:<6} {lts}{eol}", m.release_id);
        }
    }
    Ok(ExitCode::SUCCESS)
}

pub fn db_install(wanted: &str) -> io::Result<ExitCode> {
    let started = Instant::now();
    let layout = layout()?;
    let version = ops::install_mariadb(&Http::new()?, &layout, wanted, &mut Terminal::default())?;
    println!(
        "Done in {:.1}s → {}",
        started.elapsed().as_secs_f64(),
        mariadb::version_dir(&layout, &version).display()
    );
    Ok(ExitCode::SUCCESS)
}

pub fn db_remove(wanted: &str, mut purge: bool, yes: bool) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    let plan = ops::plan_remove_mariadb(&layout, wanted)?;
    let has_data = plan.last_of_major && plan.data_dir.is_dir();
    if has_data && !yes {
        if !purge {
            purge = confirm(&format!(
                "Also delete the databases of MariaDB {} ({} in {})? [y/N] ",
                plan.major,
                mib(ops::dir_size(&plan.data_dir)),
                plan.data_dir.display()
            ))?;
        }
        if purge
            && !crate::uninstall::ask(
                &format!(
                    "This deletes every database of MariaDB {}. Type `delete` to confirm: ",
                    plan.major
                ),
                &["delete"],
            )?
        {
            println!("Nothing was changed.");
            return Ok(ExitCode::SUCCESS);
        }
    }
    ops::remove_mariadb(&layout, &plan.version, purge && has_data, &mut Terminal::default())?;
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------- Composer

/// Before every `phpev composer` command: the phars of PHPev before 1.0 become versions.
pub fn composer_prepare() -> io::Result<Layout> {
    let layout = layout()?;
    ops::adopt_legacy_composer(&layout)?;
    Ok(layout)
}

/// Installs Composer `wanted` (`2.8` or `2.8.12`; `None` for the newest
/// release); returns the version. The first one installed becomes the default.
pub fn composer_install(wanted: Option<&str>) -> io::Result<String> {
    let layout = composer_prepare()?;
    let version = ops::install_composer(&Http::new()?, &layout, wanted, &mut Terminal::default())?;
    println!(
        "Composer {version} installed: {}",
        composer::version_dir(&layout, &version).display()
    );
    Ok(version)
}

/// `phpev composer self-update`: the newest Composer, made the default.
pub fn composer_self_update() -> io::Result<ExitCode> {
    let version = composer_install(None)?;
    composer_use(&version)
}

/// `phpev composer use`: makes Composer `wanted` the default, installing it first if needed.
pub fn composer_use(wanted: &str) -> io::Result<ExitCode> {
    let layout = composer_prepare()?;
    let installed = composer::installed(&layout.composer_dir());
    if version::best_match(wanted, installed.iter().map(String::as_str)).is_none() {
        composer_install(Some(wanted))?;
    }
    let found = ops::set_default_composer(&layout, wanted)?;
    println!("Composer {found} is the default for sites that do not pick one.");
    Ok(ExitCode::SUCCESS)
}

pub fn composer_list(available: bool) -> io::Result<ExitCode> {
    let layout = composer_prepare()?;
    let config = Config::load(&layout.config_file())?;
    let installed = composer::installed(&layout.composer_dir());
    let default = ops::default_composer(&layout)?;
    println!("Installed:");
    if installed.is_empty() {
        println!("  (none) — try: phpev composer install");
    }
    for v in &installed {
        let users: Vec<&str> = config
            .sites
            .iter()
            .filter(|s| {
                s.composer
                    .as_deref()
                    .is_some_and(|w| version::best_match(w, installed.iter().map(String::as_str)) == Some(v.as_str()))
            })
            .map(|s| s.name.as_str())
            .collect();
        let mark = if default.as_deref() == Some(v.as_str()) {
            "  default"
        } else {
            ""
        };
        let users = if users.is_empty() {
            String::new()
        } else {
            format!("  used by {}", users.join(", "))
        };
        println!("  {v:<8} PHP {}+{mark}{users}", composer::min_php(v));
    }

    if available {
        println!("\nAvailable (newest of each line):");
        for v in composer::lines(&composer::released(&Http::new()?)?) {
            let mark = if installed.contains(&v) { "  installed" } else { "" };
            println!(
                "  {:<6} {v:<8} PHP {}+{mark}",
                composer::line_of(&v),
                composer::min_php(&v)
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

pub fn composer_remove(version: &str) -> io::Result<ExitCode> {
    ops::remove_composer(&composer_prepare()?, version)?;
    println!("Removed Composer {version}.");
    Ok(ExitCode::SUCCESS)
}

/// `phpev composer pin|unpin`: the site given, or the one of the current folder.
pub fn composer_pin(site: Option<String>, wanted: Option<String>) -> io::Result<ExitCode> {
    let layout = composer_prepare()?;
    let config = Config::load(&layout.config_file())?;
    let site = crate::https::find_site(&config, site.as_deref())?;
    pin_composer(&layout, &site.name, wanted)?;
    Ok(ExitCode::SUCCESS)
}

/// Pins Composer `wanted` for site `name`, installing it first if needed.
pub(crate) fn pin_composer(layout: &Layout, name: &str, wanted: Option<String>) -> io::Result<()> {
    if let Some(v) = &wanted {
        // Refuse before downloading a version the site's PHP cannot run.
        ops::check_site_composer(layout, name, v)?;
        let installed = composer::installed(&layout.composer_dir());
        if version::best_match(v, installed.iter().map(String::as_str)).is_none() {
            composer_install(Some(v))?;
        }
    }
    ops::set_site_composer(layout, name, wanted.clone())?;
    match wanted {
        Some(v) => println!("{name} now runs Composer {v}."),
        None => println!("{name} now runs the default Composer."),
    }
    Ok(())
}

pub fn composer_which() -> io::Result<ExitCode> {
    let layout = composer_prepare()?;
    let cwd = std::env::current_dir()?;
    let resolved = match shim::resolve(layout.root(), &cwd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return Ok(ExitCode::FAILURE);
        }
    };
    let picked = composer::pick(
        &layout.composer_dir(),
        &resolved.version,
        resolved.composer.as_deref(),
        resolved.composer_default.as_deref(),
    );
    match picked {
        Ok(p) => {
            println!("{} (with PHP {})", p.phar.display(), resolved.version);
            match p.why {
                composer::Why::Site => println!("  Composer {} picked for the linked site", p.version),
                composer::Why::Default => println!("  Composer {}, the default (phpev composer use)", p.version),
                composer::Why::Newest => println!("  Composer {}, the newest installed (no default)", p.version),
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("{e}");
            Ok(ExitCode::FAILURE)
        }
    }
}
