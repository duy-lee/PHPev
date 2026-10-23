use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use phpev_core::config::Config;
use phpev_core::doctor::{self, Check, Status};
use phpev_core::elevate;
use phpev_core::env_path::{self, Scope};
use phpev_core::layout::{HomeSource, Layout, PathRisk, FALLBACK_HOME};
use phpev_core::HOME_ENV;

#[derive(Parser)]
#[command(
    name = "phpev",
    version,
    about = "Lightweight local PHP/Laravel environment for Windows"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create the PHPev home, put the shims on PATH and check the machine
    Setup {
        /// Use this directory as the PHPev home instead of %USERPROFILE%\.phpev
        #[arg(long, value_name = "DIR")]
        home: Option<PathBuf>,
    },
    /// Check the machine for problems
    Doctor {
        /// Put the shims first on the system PATH when another PHP/Composer shadows them (asks for admin)
        #[arg(long)]
        fix_path: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Setup { home } => setup(home),
        Command::Doctor { fix_path } => doctor(fix_path),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn setup(home: Option<PathBuf>) -> io::Result<ExitCode> {
    let previous = Layout::resolve()?;
    let layout = match home {
        Some(dir) => {
            if !dir.is_absolute() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--home must be an absolute path",
                ));
            }
            env_path::set_raw(Scope::User, HOME_ENV, &dir.to_string_lossy())?;
            Layout::at(dir, HomeSource::UserEnv)
        }
        None => previous.clone(),
    };

    if layout.source() == HomeSource::Default && !PathRisk::of(layout.root()).is_clean() {
        println!(
            "note: {} contains spaces or non-ASCII characters, which can break MariaDB and some PHP extensions.\n      \
             Consider: phpev setup --home {FALLBACK_HOME}\n",
            layout.root().display()
        );
    }

    layout.create_dirs()?;
    let config_file = layout.config_file();
    if !config_file.exists() {
        Config::default().save(&config_file)?;
    }

    let path_changed = put_shims_first_on_user_path(&layout, &previous)?;
    if path_changed {
        env_path::broadcast_change();
    }

    println!("PHPev home: {}\n", layout.root().display());
    let worst = report(&layout)?;
    if path_changed {
        println!("\nUser PATH updated. Open a new terminal for it to take effect.");
    }
    Ok(exit_code(worst))
}

/// Moves the shims to the front of the user PATH, dropping the previous home's
/// shims if the home moved. Returns whether the PATH changed.
fn put_shims_first_on_user_path(layout: &Layout, previous: &Layout) -> io::Result<bool> {
    let raw = env_path::get_raw(Scope::User, "Path")?.unwrap_or_default();
    let mut updated = raw.clone();
    if previous.root() != layout.root() {
        let old = previous.shims().to_string_lossy().into_owned();
        updated = env_path::split(&updated)
            .into_iter()
            .filter(|p| !env_path::same_dir(p, &old))
            .collect::<Vec<_>>()
            .join(";");
    }
    updated = env_path::prepend(&updated, &layout.shims().to_string_lossy());
    if updated == raw {
        return Ok(false);
    }
    env_path::set_raw(Scope::User, "Path", &updated)?;
    Ok(true)
}

fn doctor(fix_path: bool) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    if !fix_path {
        return Ok(exit_code(report(&layout)?));
    }

    let config = Config::load(&layout.config_file())?;
    let (_, path_report) = doctor::run(&layout, &config);
    if !path_report.shadowed_by_system {
        println!("Nothing on the system PATH shadows the PHPev shims; no change needed.\n");
        return Ok(exit_code(report(&layout)?));
    }

    println!("Asking for administrator rights to put the shims first on the system PATH…");
    let helper = elevate::helper_path()?;
    let shims = layout.shims();
    let code = elevate::run_elevated(&helper, &[elevate::CMD_SYSTEM_PATH_PREPEND, &shims.to_string_lossy()])?;
    if code != 0 {
        return Err(io::Error::other(format!(
            "{} failed with exit code {code}",
            elevate::HELPER_EXE
        )));
    }
    println!("System PATH updated. Open a new terminal for it to take effect.\n");
    Ok(exit_code(report(&layout)?))
}

/// Runs all checks, prints them and returns the worst status.
fn report(layout: &Layout) -> io::Result<Status> {
    let config = Config::load(&layout.config_file())?;
    let (checks, _) = doctor::run(layout, &config);
    for check in &checks {
        print_check(check);
    }
    Ok(checks.iter().map(|c| c.status).max().unwrap_or(Status::Ok))
}

fn print_check(check: &Check) {
    let tag = match check.status {
        Status::Ok => "[ ok ]",
        Status::Warn => "[warn]",
        Status::Fail => "[FAIL]",
    };
    println!("{tag} {}", check.title);
    for line in &check.details {
        println!("         {line}");
    }
    if let Some(hint) = &check.hint {
        println!("         → {hint}");
    }
}

fn exit_code(worst: Status) -> ExitCode {
    if worst == Status::Fail {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
