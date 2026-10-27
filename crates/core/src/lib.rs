//! Shared building blocks for the PHPev binaries (CLI, daemon, shims, elevate helper).

pub mod api;
pub mod autostart;
pub mod ca;
pub mod clock;
pub mod composer;
pub mod config;
pub mod constraint;
pub mod dbdump;
pub mod doctor;
pub mod editors;
pub mod elevate;
pub mod env_path;
pub mod envfile;
pub mod extract;
pub mod hosts;
pub mod installer;
pub mod ipc;
pub mod launch;
pub mod layout;
pub mod mariadb;
pub mod net;
pub mod ops;
pub mod php;
pub mod ports;
pub mod process;
pub mod procinfo;
pub mod project;
pub mod registry;
pub mod selfinstall;
pub mod shim;
pub mod sites;
pub mod state;
pub mod trust;
pub mod update;
pub mod version;
mod wide;
pub mod winsync;

/// Name of the per-user environment variable that overrides the PHPev home directory.
pub const HOME_ENV: &str = "PHPEV_HOME";
