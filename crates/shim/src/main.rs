//! `php.exe` / `composer.exe` shims that pick the right PHP version per project (milestone M3).
//! Not installed into the shims directory until it is implemented.

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("phpev-shim: not implemented yet (milestone M3)");
    ExitCode::FAILURE
}
