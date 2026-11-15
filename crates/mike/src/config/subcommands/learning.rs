use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};

use crate::analysis::embeddings::EmbeddingService;
use crate::config::{CliOverrides, load};
use crate::lesson_promotion::{CoverageSettings, Environment, Options, acquire_lock, promote};
use crate::lessons_settings::LessonWorkerConfig;
use crate::semantic::SemanticConfig;
use crate::store::persistence::PersistHandle;

pub const CONFLICTS_EXIT: i32 = 3;

#[derive(Debug, Clone, Subcommand)]
pub enum LearningCommand {
    Promote(PromoteArgs),
}

#[derive(Debug, Clone, Args)]
pub struct PromoteArgs {
    #[arg(long)]
    pub dry_run: bool,
}

fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .context("HOME is not set, so the promotion state folder is unknown")
}

fn has_scripts(candidate: &std::path::Path) -> bool {
    candidate.join("scripts/tool-reuse").is_file()
        || candidate.join("scripts/learning-promote").is_file()
}

fn package_shares(exe: &std::path::Path) -> Vec<PathBuf> {
    [Some(exe.to_path_buf()), std::fs::canonicalize(exe).ok()]
        .into_iter()
        .flatten()
        .filter_map(|bin| bin.parent().map(std::path::Path::to_path_buf))
        .flat_map(|directory| {
            [
                Some(directory.clone()),
                directory.parent().map(std::path::Path::to_path_buf),
            ]
            .into_iter()
            .flatten()
            .map(|prefix| prefix.join("share").join("mike"))
            .collect::<Vec<_>>()
        })
        .collect()
}

fn scripts_root() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("MIKE_SCRIPTS_ROOT").filter(|root| !root.is_empty()) {
        return Some(PathBuf::from(root));
    }
    let cwd = std::env::current_dir().ok();
    let exe = std::env::current_exe().ok();
    let from_cwd = cwd
        .iter()
        .flat_map(|dir| dir.ancestors())
        .map(PathBuf::from);
    let real_exe = exe
        .as_deref()
        .and_then(|exe| std::fs::canonicalize(exe).ok());
    let from_exe = real_exe
        .iter()
        .flat_map(|exe| exe.ancestors())
        .map(PathBuf::from);
    let shares = exe.iter().flat_map(|exe| package_shares(exe));
    from_cwd
        .chain(from_exe)
        .chain(shares)
        .find(|candidate| has_scripts(candidate))
}

async fn checkout_root(persist: &PersistHandle, machine: uuid::Uuid) -> Result<Option<PathBuf>> {
    let locations = persist
        .locations_on_machine(machine)
        .await
        .context("could not read this machine's checkouts")?;
    Ok(locations
        .into_iter()
        .map(|location| PathBuf::from(location.root_path))
        .find(|root| has_scripts(root)))
}

async fn environment(persist: &PersistHandle) -> Result<Environment> {
    let identity = crate::machine_identity::load_default()
        .await
        .context("could not read this machine's identity")?;
    let root = match scripts_root() {
        Some(root) => root,
        None => checkout_root(persist, identity.machine_uuid).await?.context(
            "could not find the mike scripts folder beside the binary, in the working directory or in a checkout this machine registered; set MIKE_SCRIPTS_ROOT to the folder that holds scripts/",
        )?,
    };
    Ok(Environment {
        home: home_dir()?,
        root,
        machine_uuid: identity.machine_uuid,
        machine_name: identity.machine_name,
    })
}

async fn run_promote(args: PromoteArgs, cli: CliOverrides) -> Result<()> {
    let mut config = load::load(cli).context("failed to resolve config for learning promote")?;
    let Some(database_url) = config.database_url.clone() else {
        anyhow::bail!(
            "learning promote requires a database_url (set it in /settings or MIKE_DATABASE_URL)"
        );
    };
    let persist = PersistHandle::connect_existing(&database_url)
        .await
        .context("could not connect to the target database")?;
    let rows = persist.fetch_settings().await?;
    crate::config::apply_settings_overrides(&mut config, &rows);
    let environment = environment(&persist).await?;
    let _lock = acquire_lock(&environment.home)?;
    let embedding = EmbeddingService::new(&config.embedding);
    let coverage = CoverageSettings {
        service: &embedding,
        threshold: f64::from(config.sections.get::<LessonWorkerConfig>().dedup_threshold),
        allow_remote: SemanticConfig::from_rows(&rows).allow_remote_embedding,
    };
    let report = promote(
        &persist,
        Some(&coverage),
        &environment,
        &Options {
            dry_run: args.dry_run,
        },
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&report.result)?);
    if report.has_conflicts {
        std::process::exit(CONFLICTS_EXIT);
    }
    Ok(())
}

pub async fn run(command: LearningCommand, cli: CliOverrides) -> Result<()> {
    match command {
        LearningCommand::Promote(args) => run_promote(args, cli).await,
    }
}
