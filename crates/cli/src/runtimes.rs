//! `phpev php …`, `phpev db …` and `phpev composer …`.

use std::io;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use phpev_core::config::Config;
use phpev_core::layout::Layout;
use phpev_core::net::Http;
use phpev_core::{clock, composer, mariadb, ops, php, shim, version};

use crate::progress::{Bar, Terminal};

/// Loads the layout, creating its directories if `phpev setup` has not run yet.
fn layout() -> io::Result<Layout> {
    let layout = Layout::resolve()?;
    layout.create_dirs()?;
    Ok(layout)
}

fn with_bar<T>(label: &str, f: impl FnOnce(&mut dyn FnMut(u64, Option<u64>)) -> io::Result<T>) -> io::Result<T> {
    let mut bar = Bar::new(label);
    let result = f(&mut |done, total| bar.update(done, total));
    bar.finish();
    result
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

// ---------------------------------------------------------------- Composer

pub fn composer_install(update: bool) -> io::Result<ExitCode> {
    let layout = layout()?;
    if composer::is_installed(&layout) && !update {
        println!(
            "Composer is already installed: {}",
            composer::phar_path(&layout).display()
        );
        println!("Use `phpev composer self-update` to get the latest version.");
        return Ok(ExitCode::SUCCESS);
    }
    let http = Http::new()?;
    with_bar("composer.phar", |p| composer::install_latest(&http, &layout, p))?;
    println!("Composer installed: {}", composer::phar_path(&layout).display());
    Ok(ExitCode::SUCCESS)
}
