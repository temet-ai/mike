use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::config::{CliOverrides, paths::InstallationPaths};

#[derive(Debug, Clone, Args)]
pub struct ImportDotenvArgs {
    pub source: PathBuf,
    #[arg(long)]
    pub ignore_unknown: bool,
}

pub(super) fn run(args: ImportDotenvArgs, cli: CliOverrides) -> Result<()> {
    let paths = InstallationPaths::resolve(cli.config.as_deref())?;
    if paths.config_file.exists() {
        anyhow::ensure!(
            std::fs::canonicalize(&args.source)? != std::fs::canonicalize(&paths.config_file)?,
            "dotenv source and selected configuration must be different files"
        );
    }
    let assignments = crate::config::load::dotenv::read(&args.source)?;
    let (table, skipped) = crate::config::load::import::convert(assignments, args.ignore_unknown)?;
    let backup = crate::config::write::update_selected(&paths.config_file, table)?;
    println!("Imported literal dotenv values into the selected configuration. Source unchanged.");
    if backup.is_some() {
        println!("Previous configuration backed up beside the selected file.");
    }
    if skipped > 0 {
        println!("Skipped {skipped} unsupported variables as requested.");
    }
    Ok(())
}
