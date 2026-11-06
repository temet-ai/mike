mod commands;
mod compose;
mod credentials;
mod down;
mod entry;
mod error;
mod native;
mod runner;
mod server;
mod status;
mod up;

#[cfg(test)]
mod tests;

use clap::{Args, Subcommand};

use crate::config::CliOverrides;

pub(crate) use compose::{Daemon, VOLUME, daemon as docker_daemon};
pub(crate) use down::{DownOutcome, down, remove_volume};
pub(crate) use entry::start_brew_database;
pub(crate) use error::DbError;
pub(crate) use native::{Platform as NativePlatform, detect as detect_native, plan as native_plan};
pub(crate) use runner::{Call, Captured, Runner, SystemRunner};
pub(crate) use up::{Paths, Timing};

#[derive(Debug, Clone, Args)]
pub struct DbArgs {
    #[command(subcommand)]
    pub command: DbCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum DbCommand {
    Up(UpArgs),
    Check(CheckArgs),
    Down,
    Status,
    Native(NativeArgs),
}

#[derive(Debug, Clone, Args)]
pub struct UpArgs {
    #[arg(long)]
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Args)]
pub struct CheckArgs {
    pub url: Option<String>,
    #[arg(long, requires = "url")]
    pub save: bool,
}

#[derive(Debug, Clone, Args)]
pub struct NativeArgs {
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(args: DbArgs, overrides: CliOverrides) -> i32 {
    commands::run(args, overrides).await
}
