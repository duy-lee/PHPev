//! Shared building blocks for the PHPev binaries (CLI, daemon, shims, elevate helper).

pub mod clock;
pub mod composer;
pub mod config;
pub mod constraint;
pub mod doctor;
pub mod elevate;
pub mod env_path;
pub mod envfile;
pub mod extract;
pub mod hosts;
pub mod ipc;
pub mod layout;
pub mod mariadb;
pub mod net;
pub mod php;
pub mod ports;
pub mod procinfo;
pub mod registry;
pub mod selfinstall;
pub mod shim;
pub mod sites;
pub mod state;
pub mod trust;
pub mod version;
mod wide;
pub mod winsync;

/// Name of the per-user environment variable that overrides the PHPev home directory.
pub const HOME_ENV: &str = "PHPEV_HOME";
