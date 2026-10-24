//! `phpev uninstall`: undo everything PHPev changed on this computer.
//!
//! Keeps the databases (`data\`) and `config.toml` unless `--purge`, so a slip
//! of the keyboard cannot lose data. Admin-level changes (hosts file, system
//! PATH, Defender exclusions) are undone together behind one UAC prompt.

use std::io::{self, BufRead, IsTerminal, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use phpev_core::config::Config;
use phpev_core::elevate::{self, CMD_UNINSTALL_CLEANUP};
use phpev_core::env_path::{self, Scope};
use phpev_core::layout::Layout;
use phpev_core::{autostart, ca, hosts, ipc, HOME_ENV};

use crate::daemon;

/// What stays in the home without `--purge`.
const KEPT: &[&str] = &["data", "config.toml"];

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;

pub fn uninstall(purge: bool, yes: bool) -> io::Result<ExitCode> {
    let layout = Layout::resolve()?;
    let root = layout.root().to_path_buf();
    let shims = layout.shims();
    let shims_str = shims.to_string_lossy().into_owned();
    let config = Config::load(&layout.config_file()).unwrap_or_default();

    let hosts_block = hosts_file_text().contains(hosts::BEGIN);
    let system_path =
        env_path::get_raw(Scope::System, "Path")?.is_some_and(|raw| env_path::position(&raw, &shims_str).is_some());
    let mut defender: Vec<PathBuf> = Vec::new();
    if config.defender_excluded {
        defender.push(root.clone());
    }
    defender.extend(
        config
            .sites
            .iter()
            .filter(|s| s.defender_excluded)
            .map(|s| s.path.clone()),
    );
    let needs_admin = hosts_block || system_path || !defender.is_empty();
    let ca_der = ca::stored_der(&layout);

    println!("This removes PHPev from this computer:");
    println!("  - stops PHPev, MariaDB and the PHP workers");
    println!("  - turns off start at log in");
    if ca_der.is_some() {
        println!("  - removes the PHPev certificate authority used for HTTPS (Windows asks to confirm)");
    }
    println!("  - removes PHPev from your PATH");
    if needs_admin {
        println!("  - asks once for admin rights (UAC) to remove:");
        if hosts_block {
            println!("      the PHPev entries in the hosts file");
        }
        if system_path {
            println!("      {shims_str} from the system PATH");
        }
        for dir in &defender {
            println!("      the Microsoft Defender exclusion for {}", dir.display());
        }
    }
    if purge {
        println!("  - deletes {} entirely, INCLUDING YOUR DATABASES", root.display());
    } else {
        println!(
            "  - deletes {} except your databases (data\\) and config.toml (--purge deletes them too)",
            root.display()
        );
    }
    println!("Your project folders are not touched.\n");

    if !yes {
        if !io::stdin().is_terminal() {
            return Err(io::Error::other("no terminal to confirm in; re-run with --yes"));
        }
        if !ask("Uninstall PHPev? [y/N] ", &["y", "yes"])? {
            println!("Nothing was changed.");
            return Ok(ExitCode::SUCCESS);
        }
        if purge
            && !ask(
                "This deletes every database of PHPev's MariaDB. Type `delete` to confirm: ",
                &["delete"],
            )?
        {
            println!("Nothing was changed.");
            return Ok(ExitCode::SUCCESS);
        }
    }

    let mut problems: Vec<String> = Vec::new();

    // Stopping first releases the files (MariaDB shuts down cleanly, the
    // dashboard window closes itself).
    if ipc::is_running() {
        daemon::stop()?;
    }

    if let Err(e) = autostart::set(&layout, false) {
        problems.push(format!("Could not turn off start at log in: {e}"));
    }

    if let Some(der) = ca_der {
        let removed = ca::is_trusted(&der).and_then(|trusted| {
            if trusted {
                println!("Windows will ask you to confirm removing the PHPev CA…");
                ca::untrust(&der)?;
            }
            Ok(())
        });
        if let Err(e) = removed {
            problems.push(format!(
                "The PHPev CA is still trusted ({e}). Remove \"PHPev Local Development CA\" in certmgr.msc → Trusted Root Certification Authorities."
            ));
        }
    }

    if needs_admin {
        let mut args: Vec<String> = vec![CMD_UNINSTALL_CLEANUP.to_owned()];
        if hosts_block {
            args.push("--hosts".to_owned());
        }
        if system_path {
            args.extend(["--system-path".to_owned(), shims_str.clone()]);
        }
        for dir in &defender {
            args.extend(["--defender".to_owned(), dir.to_string_lossy().into_owned()]);
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        println!("Asking for admin rights…");
        match elevate::helper_path().and_then(|helper| elevate::run_elevated(&helper, &args)) {
            Ok(0) => println!("Removed the hosts entries, system PATH entry and Defender exclusions."),
            Ok(_) | Err(_) => {
                if hosts_block {
                    problems.push(format!(
                        "Remove the block between \"{}\" and \"{}\" from C:\\Windows\\System32\\drivers\\etc\\hosts (as admin).",
                        hosts::BEGIN,
                        hosts::END
                    ));
                }
                if system_path {
                    problems.push(format!("Remove {shims_str} from the system PATH (as admin)."));
                }
                for dir in &defender {
                    problems.push(format!(
                        "Remove the Defender exclusion in an admin PowerShell: Remove-MpPreference -ExclusionPath '{}'",
                        dir.display()
                    ));
                }
            }
        }
    }

    match env_path::get_raw(Scope::User, "Path") {
        Ok(Some(raw)) => {
            let updated = env_path::remove(&raw, &shims_str);
            if updated != raw {
                if let Err(e) = env_path::set_raw(Scope::User, "Path", &updated) {
                    problems.push(format!("Remove {shims_str} from your user PATH ({e})."));
                }
            }
        }
        Ok(None) => {}
        Err(e) => problems.push(format!("Could not read your user PATH ({e}).")),
    }
    // A custom home stays findable while its data is kept.
    if purge {
        let _ = env_path::delete(Scope::User, HOME_ENV);
    }
    env_path::broadcast_change();

    delete_files(&root, purge, &mut problems);

    println!();
    if problems.is_empty() {
        println!("PHPev was removed. Open a new terminal: `php` and `composer` no longer point to PHPev.");
    } else {
        println!("PHPev was removed, but a few things need your attention:");
        for p in &problems {
            println!("  - {p}");
        }
    }
    if !purge && root.exists() {
        println!(
            "Kept: {} (databases and config.toml). Reinstalling PHPev uses them again; delete the folder to remove them.",
            root.display()
        );
    }
    Ok(if problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Deletes the home (everything, or all but `KEPT`). The running `phpev.exe`
/// usually lives in it and cannot be deleted while running, so it is moved to
/// the temp folder first and deleted from there once this process has exited.
fn delete_files(root: &Path, purge: bool, problems: &mut Vec<String>) {
    if let Ok(exe) = std::env::current_exe() {
        if exe.starts_with(root) {
            let moved = std::env::temp_dir().join(format!("phpev-uninstall-{}.exe", std::process::id()));
            match std::fs::rename(&exe, &moved) {
                Ok(()) => delete_after_exit(&moved),
                Err(e) => problems.push(format!("{} is in use ({e}); delete it by hand.", exe.display())),
            }
        }
    }
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !purge && KEPT.iter().any(|k| k.eq_ignore_ascii_case(&name)) {
            continue;
        }
        let path = entry.path();
        let result = if entry.file_type().is_ok_and(|t| t.is_dir()) {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if let Err(e) = result {
            problems.push(format!(
                "Could not delete {} ({e}); a program may still use it (close terminals running `php`), then delete it.",
                path.display()
            ));
        }
    }
    if purge {
        let _ = std::fs::remove_dir(root);
    }
}

/// Deletes `file` a few seconds after this process exits, via a hidden `cmd`.
/// Skipped (the file stays in the temp folder) if the path would need quoting
/// tricks that cmd cannot do safely.
fn delete_after_exit(file: &Path) {
    let path = file.to_string_lossy();
    if path.chars().any(|c| "\"%&^|<>!".contains(c)) {
        return;
    }
    let system_root = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let _ = Command::new(system_root.join(r"System32\cmd.exe"))
        .raw_arg(format!("/d /c ping -n 4 127.0.0.1 >nul & del /f /q \"{path}\""))
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn hosts_file_text() -> String {
    let system_root = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    std::fs::read(system_root.join(r"System32\drivers\etc\hosts"))
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}

fn ask(prompt: &str, accept: &[&str]) -> io::Result<bool> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(accept.contains(&line.trim().to_ascii_lowercase().as_str()))
}
