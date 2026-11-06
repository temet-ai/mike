use anyhow::{Context, Result};
use clap::{Args, Subcommand};

use crate::config::{ProxyConfig, paths::InstallationPaths};

#[derive(Debug, Clone, Args)]
pub struct DatabaseArgs {
    #[command(subcommand)]
    pub command: DatabaseCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum DatabaseCommand {
    #[command(about = "Validate PostgreSQL prerequisites without running migrations")]
    Check,
    #[command(about = "Provision an owned Docker database and save the selected private config")]
    Provision {
        #[arg(long, default_value_t = 6666)]
        port: u16,
        #[arg(long)]
        image: Option<String>,
    },
    #[command(about = "Stop this installation’s owned Docker database without deleting data")]
    Stop,
}

pub async fn run(args: DatabaseArgs, config: &ProxyConfig) -> Result<()> {
    let paths = InstallationPaths::resolve(Some(&config.config_path))?;
    match args.command {
        DatabaseCommand::Check => {
            let url = config.database_url.as_deref().context(
                "no database URL selected; configure an existing PostgreSQL database first",
            )?;
            let ready = super::database::validate_existing(url).await?;
            println!(
                "PostgreSQL {} prerequisites passed; {} embedded migrations pending. Only mike serve applies migrations.",
                ready.server_major, ready.pending_migrations
            );
        }
        DatabaseCommand::Provision { port, image } => {
            super::plan::validate_overrides(config, true, None)?;
            let image = image.or_else(|| std::env::var("MIKE_DOCKER_DB_IMAGE").ok());
            let url = super::docker::provision(&paths, port, image.as_deref()).await?;
            super::plan::validate_overrides(config, true, Some(&url))?;
            super::persistence::save_database_mode_at(&paths.config_file, Some(&url))?;
            let reloaded = crate::config::reload_selected(config)?;
            anyhow::ensure!(
                reloaded.persist_enabled
                    && !reloaded.db_fallback_enabled
                    && reloaded.database_url.as_deref() == Some(url.as_str()),
                "database configuration was saved, but an environment or CLI override selects another mode, fallback policy or URL; remove the conflicting override before starting"
            );
            println!(
                "Managed PostgreSQL prerequisites passed and the selected config was saved. Run mike serve to apply embedded migrations. Database credentials were not printed."
            );
        }
        DatabaseCommand::Stop => {
            super::docker::stop(&paths).await?;
            println!(
                "Owned managed database stopped; its persistent volume and credential were preserved."
            );
        }
    }
    Ok(())
}
