use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use clap::{Args, ValueEnum};

use crate::config::CliOverrides;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum StorageMode {
    Trial,
    Docker,
    Existing,
}

#[derive(Debug, Clone, Args)]
#[command(about = "Configure and verify the selected installation using the guided setup backend")]
pub struct SetupArgs {
    #[arg(conflicts_with = "mode")]
    pub step: Option<String>,
    #[arg(long, value_enum)]
    pub mode: Option<StorageMode>,
    #[arg(
        long,
        help = "Owner-only file containing the existing database URL; never pass passwords as CLI arguments"
    )]
    pub database_url_file: Option<PathBuf>,
    #[arg(long, default_value_t = 6666)]
    pub port: u16,
    #[arg(long, help = "Optional digest-pinned managed PostgreSQL image")]
    pub image: Option<String>,
    #[arg(
        long,
        help = "Acknowledge possible owned-server restart and loss of unpersisted trial captures"
    )]
    pub accept_trial_loss: bool,
    #[arg(long)]
    pub json: bool,
}

pub async fn run(args: SetupArgs, overrides: CliOverrides) -> Result<()> {
    let Some(mode) = args.mode else {
        ensure!(
            args.database_url_file.is_none()
                && args.image.is_none()
                && !args.accept_trial_loss
                && !args.json,
            "headless setup options require --mode"
        );
        return crate::tui::run_setup(overrides, args.step).await;
    };
    ensure!(
        args.accept_trial_loss,
        "{} Repeat with --accept-trial-loss to proceed.",
        super::plan::TRIAL_RESTART_WARNING
    );
    let mut config = crate::config::load::load(overrides)?;
    let choice = match mode {
        StorageMode::Trial => {
            ensure!(
                args.database_url_file.is_none() && args.image.is_none(),
                "trial setup does not accept database URL files or Docker images"
            );
            super::plan::StorageChoice::Trial
        }
        StorageMode::Docker => {
            ensure!(
                args.database_url_file.is_none(),
                "Docker setup creates a private credential; do not supply a database URL file"
            );
            super::plan::StorageChoice::Docker {
                port: args.port,
                image: args.image,
            }
        }
        StorageMode::Existing => {
            ensure!(
                args.image.is_none(),
                "existing database setup does not accept a Docker image"
            );
            let url = match args.database_url_file {
                Some(path) => read_url_file(&path)?,
                None => std::env::var("MIKE_DATABASE_URL").context("existing setup requires --database-url-file or an explicitly exported MIKE_DATABASE_URL")?,
            };
            super::plan::StorageChoice::Existing(url)
        }
    };
    let report = super::plan::apply(&mut config, choice).await?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "Setup verified: {} storage; selected config {}",
            if report.persistent {
                "persistent"
            } else {
                "ephemeral trial"
            },
            report.config_file.display()
        );
        if let Some(backup) = report.previous_config_backup {
            println!("Previous configuration retained at {}", backup.display());
        }
        println!("{}", super::plan::TRIAL_RESTART_WARNING);
        println!(
            "Usage options: mike claude; standalone proxy; mike mcp register; browser UI. Run mike doctor --json to inspect the selected installation."
        );
    }
    Ok(())
}

fn read_url_file(path: &Path) -> Result<String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .context("cannot open the protected database URL file")?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= 65536,
        "database URL input must be a regular file of at most 64 KiB"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        ensure!(
            metadata.uid() == rustix::process::geteuid().as_raw()
                && metadata.permissions().mode() & 0o077 == 0,
            "database URL file must belong to this user with owner-only permissions (chmod 600)"
        );
    }
    use std::io::Read;
    let mut value = String::new();
    file.take(65537)
        .read_to_string(&mut value)
        .context("database URL file must contain UTF-8 text")?;
    ensure!(value.len() <= 65536, "database URL input exceeds 64 KiB");
    let value = value.trim_end_matches(['\r', '\n']).to_string();
    ensure!(!value.is_empty(), "database URL file is empty");
    Ok(value)
}
