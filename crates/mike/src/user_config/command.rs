use std::path::PathBuf;

use anyhow::Context;
use clap::{Args, Subcommand};

use super::bootstrap::resolved_config_file;
use super::import::import_env;

#[derive(Debug, Clone, Args)]
pub struct UserConfigArgs {
    #[command(subcommand)]
    pub command: UserConfigCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum UserConfigCommand {
    Path {
        #[arg(long)]
        state: bool,
    },
    ImportEnv(ImportEnvArgs),
}

#[derive(Debug, Clone, Args)]
pub struct ImportEnvArgs {
    #[arg(default_value = ".env")]
    pub path: PathBuf,
}

pub fn run(args: UserConfigArgs) -> anyhow::Result<()> {
    match args.command {
        UserConfigCommand::Path { state: true } => {
            let selected = resolved_config_file()?;
            let paths = crate::config::paths::InstallationPaths::resolve(Some(&selected))?;
            println!("{}", paths.state_dir.display());
            Ok(())
        }
        UserConfigCommand::Path { state: false } => {
            println!("{}", resolved_config_file()?.display());
            Ok(())
        }
        UserConfigCommand::ImportEnv(args) => import_command(&args),
    }
}

fn import_command(args: &ImportEnvArgs) -> anyhow::Result<()> {
    let config_path = resolved_config_file()?;
    let text = std::fs::read_to_string(&args.path)
        .with_context(|| format!("cannot read {}", args.path.display()))?;
    let report = import_env(&text, &config_path)?;
    println!(
        "{}: {} imported, {} kept as they were, {} lines ignored (not MIKE_*)",
        config_path.display(),
        report.imported.len(),
        report.kept.len(),
        report.ignored
    );
    for key in &report.imported {
        println!("  imported {key}");
    }
    for key in &report.kept {
        println!("  kept     {key}");
    }
    Ok(())
}
