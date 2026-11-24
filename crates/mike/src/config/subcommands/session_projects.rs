use std::collections::BTreeMap;

use anyhow::{Context, Result};
use clap::Args;

use crate::config::{CliOverrides, load};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::unresolved_session_backfill::ProposedSessionProject;
use crate::workspace::checkouts::project_for_repo;
use crate::workspace::git_repo::repo_identity;

#[derive(Debug, Clone, Args)]
pub struct ResolveSessionProjectsArgs {
    #[arg(long)]
    pub execute: bool,
    #[arg(long = "rule")]
    pub rules: Vec<String>,
    #[arg(long, default_value_t = 5)]
    pub examples: usize,
}

pub(super) async fn resolve_session_projects(
    args: ResolveSessionProjectsArgs,
    cli: CliOverrides,
) -> Result<()> {
    let config = load::load(cli).context("failed to resolve config for resolve-session-projects")?;
    let database_url = config
        .database_url
        .as_deref()
        .context("resolve-session-projects requires a database_url (MIKE_DATABASE_URL)")?;
    let handle = PersistHandle::connect_existing(database_url)
        .await
        .context("resolve-session-projects: could not connect to the database")?;
    let wright_project = mike_checkout_project(&handle).await?;
    place_home_roots(&handle, args.execute).await?;
    let proposals: Vec<ProposedSessionProject> = handle
        .propose_unresolved_session_projects(wright_project)
        .await?
        .into_iter()
        .filter(|p| args.rules.is_empty() || args.rules.contains(&p.rule))
        .collect();
    report(&proposals, args.examples);
    if !args.execute {
        println!("dry run: nothing written; pass --execute to apply");
        return Ok(());
    }
    for proposal in &proposals {
        handle
            .apply_proposed_session_project(proposal)
            .await
            .with_context(|| format!("could not resolve session {}", proposal.session_uuid))?;
    }
    println!("applied {} session project(s)", proposals.len());
    Ok(())
}

async fn place_home_roots(handle: &PersistHandle, execute: bool) -> Result<()> {
    let roots = handle.unresolved_home_roots().await?;
    let verb = if execute { "placing" } else { "would place" };
    for root in &roots {
        println!(
            "{verb} {} session(s) under {} on {} in project '{} home'",
            root.sessions, root.home_root, root.machine_name, root.machine_name
        );
        if execute {
            handle
                .ensure_machine_home_location(root.machine_uuid, &root.home_root)
                .await?;
        }
    }
    Ok(())
}

async fn mike_checkout_project(handle: &PersistHandle) -> Result<Option<i64>> {
    let Some(identity) = std::env::current_dir()
        .ok()
        .and_then(|dir| repo_identity(&dir))
        .filter(|identity| identity.main_root.join("crates/mike/Cargo.toml").is_file())
    else {
        println!("not run from mike's checkout: Wright engineer sessions are left out");
        return Ok(None);
    };
    let machine = crate::machine_identity::load_default()
        .await
        .context("this machine has no mike identity")?
        .machine_uuid;
    Ok(project_for_repo(handle, machine, &identity).await?)
}

fn report(proposals: &[ProposedSessionProject], examples: usize) {
    let mut by_rule: BTreeMap<&str, BTreeMap<&str, usize>> = BTreeMap::new();
    for proposal in proposals {
        *by_rule
            .entry(proposal.rule.as_str())
            .or_default()
            .entry(proposal.project_path.as_str())
            .or_default() += 1;
    }
    println!("{} unresolved session(s) have a project to take", proposals.len());
    for (rule, projects) in &by_rule {
        println!("rule {rule}: {}", projects.values().sum::<usize>());
        for (project, count) in projects {
            println!("  {count:>5}  {project}");
        }
        for proposal in proposals.iter().filter(|p| p.rule == *rule).take(examples) {
            println!(
                "    e.g. {}  cwd={}  -> {}",
                proposal.session_uuid,
                proposal.cwd.as_deref().unwrap_or("(none)"),
                proposal.project_path
            );
        }
    }
}
