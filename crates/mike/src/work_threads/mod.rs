pub mod cli;
pub(crate) mod export;
mod import;

pub use cli::{ImportArgs, ThreadArgs, ThreadCommand};

use crate::config::CliOverrides;

pub async fn run(args: ThreadArgs, overrides: CliOverrides) -> anyhow::Result<()> {
    match args.command {
        ThreadCommand::Import(import_args) => import::run(import_args, overrides).await,
    }
}
