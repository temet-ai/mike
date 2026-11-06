use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Clone, Args)]
pub struct ThreadArgs {
    #[command(subcommand)]
    pub command: ThreadCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ThreadCommand {
    Import(ImportArgs),
}

#[derive(Debug, Clone, Args)]
pub struct ImportArgs {
    #[arg(long = "repo", value_name = "PATH")]
    pub repos: Vec<PathBuf>,
    #[arg(long, value_name = "FILE")]
    pub registry: Option<PathBuf>,
    #[arg(long)]
    pub dry_run: bool,
}
