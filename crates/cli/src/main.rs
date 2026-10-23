mod daemon;
mod import;
mod progress;
mod runtimes;
mod sites;

use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use phpev_core::config::{self, Config};
use phpev_core::doctor::{self, Check, Status};
use phpev_core::elevate;
use phpev_core::env_path::{self, Scope};
use phpev_core::layout::{HomeSource, Layout, PathRisk, FALLBACK_HOME};
use phpev_core::{selfinstall, shim, HOME_ENV};

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
        /// Add a Microsoft Defender exclusion for the PHPev home (asks for admin)
        #[arg(long)]
        defender_exclude: bool,
    },
    /// Manage PHP versions
    Php {
        #[command(subcommand)]
        command: PhpCommand,
    },
    /// Manage MariaDB versions
    Db {
        #[command(subcommand)]
        command: DbCommand,
    },
    /// Manage Composer
    Composer {
        #[command(subcommand)]
        command: ComposerCommand,
    },
    /// Start the PHPev background service
    Start,
    /// Stop the background service and everything it runs
    Stop,
    /// Show what the background service is running
    Status,
    /// Start the background service when you log in to Windows
    Autostart {
        #[arg(value_enum)]
        mode: daemon::Autostart,
    },
    /// Serve every sub-folder of DIR (default: current folder) as http://<folder>.localhost
    Park { dir: Option<PathBuf> },
    /// Stop serving the sub-folders of DIR (default: current folder)
    Unpark { dir: Option<PathBuf> },
    /// Serve the current folder as http://<name>.localhost
    Link {
        /// Site name (default: the folder name)
        name: Option<String>,
        /// Pin a PHP version for this site (a `.php-version` file still wins)
        #[arg(long)]
        php: Option<String>,
    },
    /// Remove a linked site (default: the one for the current folder)
    Unlink { name: Option<String> },
    /// List all sites with their URL, PHP version and folder
    Sites,
    /// Make an existing project run: pick PHP, check extensions, composer install, .env, database, domain
    Import {
        /// Project folder (default: current folder)
        dir: Option<PathBuf>,
        /// Site name (default: the folder name)
        #[arg(long)]
        name: Option<String>,
        /// Answer yes to every question (except migrations)
        #[arg(long, short = 'y')]
        yes: bool,
        /// Also run `php artisan migrate`
        #[arg(long)]
        migrate: bool,
        /// Exclude the project folder from Microsoft Defender scanning (asks for admin; not implied by --yes)
        #[arg(long)]
        defender_exclude: bool,
    },
    /// Manage the PHPev entries in the Windows hosts file
    Hosts {
        #[command(subcommand)]
        command: HostsCommand,
    },
}

#[derive(Subcommand)]
enum HostsCommand {
    /// Write every site into the hosts file (asks for admin). Needed only when
    /// an app calls itself through PHP streams/PowerShell, or with a TLD other than localhost
    Sync,
}

#[derive(Subcommand)]
enum PhpCommand {
    /// List installed (and with --available, downloadable) versions
    List {
        #[arg(long)]
        available: bool,
    },
    /// Download and install a version, e.g. `8.4` or `8.4.27`
    Install { version: String },
    /// Set the default version used outside projects
    Use { version: String },
    /// Show (and create if missing) the php.ini of a version
    Ini { version: String },
    /// Show which PHP `php` would run in the current directory, and why
    Which,
    /// Start the php-cgi workers of a version now (default: the default version)
    Warm { version: Option<String> },
}

#[derive(Subcommand)]
enum DbCommand {
    /// List installed (and with --available, downloadable) versions
    List {
        #[arg(long)]
        available: bool,
    },
    /// Download and install the latest release of a major version, e.g. `11.4`
    Install { version: String },
    /// Start MariaDB (remembered: it comes back when PHPev starts again)
    Start,
    /// Stop MariaDB
    Stop,
    /// Switch the active major version, e.g. `10.11` (each major keeps its own databases)
    Use { version: String },
}

#[derive(Subcommand)]
enum ComposerCommand {
    /// Download composer.phar if it is not installed yet
    Install,
    /// Replace composer.phar with the latest stable release
    SelfUpdate,
}

fn main() -> ExitCode {
    // `phpev … | findstr` may close our stdout early; println! then panics. Like
    // Unix tools on SIGPIPE, just stop quietly.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .unwrap_or("");
        if msg.starts_with("failed printing to stdout") {
            std::process::exit(0);
        }
        default_hook(info);
    }));

    let cli = Cli::parse();
    let result = match cli.command {
        Command::Setup { home } => setup(home),
        Command::Doctor {
            fix_path,
            defender_exclude,
        } => doctor(fix_path, defender_exclude),
        Command::Php { command } => match command {
            PhpCommand::List { available } => runtimes::php_list(available),
            PhpCommand::Install { version } => runtimes::php_install(&version),
            PhpCommand::Use { version } => runtimes::php_use(&version),
            PhpCommand::Ini { version } => runtimes::php_ini(&version),
            PhpCommand::Which => runtimes::php_which(),
            PhpCommand::Warm { version } => daemon::php_warm(version),
        },
        Command::Db { command } => match command {
            DbCommand::List { available } => runtimes::db_list(available),
            DbCommand::Install { version } => runtimes::db_install(&version),
            DbCommand::Start => daemon::db_start(),
            DbCommand::Stop => daemon::db_stop(),
            DbCommand::Use { version } => daemon::db_use(&version),
        },
        Command::Composer { command } => match command {
            ComposerCommand::Install => runtimes::composer_install(false),
            ComposerCommand::SelfUpdate => runtimes::composer_install(true),
        },
        Command::Start => daemon::start(),
        Command::Stop => daemon::stop(),
        Command::Status => daemon::status(),
        Command::Autostart { mode } => daemon::autostart(mode),
        Command::Park { dir } => sites::park(dir),
        Command::Unpark { dir } => sites::unpark(dir),
        Command::Link { name, php } => sites::link(name, php),
        Command::Unlink { name } => sites::unlink(name),
        Command::Sites => sites::list(),
        Command::Import {
            dir,
            name,
            yes,
            migrate,
            defender_exclude,
        } => import::import(import::Options {
            dir,
            name,
            yes,
            migrate,
            defender_exclude,
        }),
        Command::Hosts {
            command: HostsCommand::Sync,
        } => sites::hosts_sync(),
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
    if config_file.exists() {
        // Refresh the shims' view of the config (e.g. after upgrading PHPev).
        shim::write_cache(&config_file, &Config::load(&config_file)?)?;
    } else {
        Config::default().save(&config_file)?;
    }

    let exe_dir = std::env::current_exe()?
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("cannot locate phpev.exe"))?;
    let installed = selfinstall::install(&layout, &exe_dir)?;
    if !installed.is_empty() {
        println!("Installed into {}: {}", layout.shims().display(), installed.join(", "));
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

fn doctor(fix_path: bool, defender_exclude: bool) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    let config_file = layout.config_file();

    if fix_path {
        let (_, path_report) = doctor::run(&layout, &Config::load(&config_file)?);
        if path_report.shadowed_by_system {
            println!("Asking for administrator rights to put the shims first on the system PATH…");
            run_helper(elevate::CMD_SYSTEM_PATH_PREPEND, &layout.shims().to_string_lossy())?;
            println!("System PATH updated. Open a new terminal for it to take effect.\n");
        } else {
            println!("Nothing on the system PATH shadows the PHPev shims; no change needed.\n");
        }
    }

    if defender_exclude {
        println!("Asking for administrator rights to add a Microsoft Defender exclusion…");
        run_helper(elevate::CMD_DEFENDER_EXCLUDE, &layout.root().to_string_lossy())?;
        config::update(&config_file, |c| c.defender_excluded = true)?;
        println!("Excluded {} from Defender scanning.\n", layout.root().display());
    }

    Ok(exit_code(report(&layout)?))
}

/// Runs one phpev-elevate subcommand through UAC.
fn run_helper(command: &str, arg: &str) -> io::Result<()> {
    let code = elevate::run_elevated(&elevate::helper_path()?, &[command, arg])?;
    if code == 0 {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{} {command} failed with exit code {code}",
            elevate::HELPER_EXE
        )))
    }
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
