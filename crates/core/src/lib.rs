//! Shared building blocks for the PHPev binaries (CLI, daemon, shims, elevate helper).

pub mod config;
pub mod doctor;
pub mod elevate;
pub mod env_path;
pub mod layout;
pub mod ports;
pub mod registry;
mod wide;

/// Name of the per-user environment variable that overrides the PHPev home directory.
pub const HOME_ENV: &str = "PHPEV_HOME";
