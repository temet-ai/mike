use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::bundle;
use super::checks::Missing;
use super::definition::FleetDefinition;
use super::plan::{ready_tasks, waiting_tasks, warm_disciplines};
use crate::launch::TerminalBackend;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Starts {
    pub seats: usize,
    pub most_at_once: usize,
    pub control_seats: usize,
    pub workers: Vec<String>,
    pub templates: Vec<String>,
    pub reference: bool,
    pub waiting: Vec<String>,
}

pub fn starts(def: &FleetDefinition) -> Starts {
    let workers = ready_tasks(def, &BTreeSet::new());
    let templates = warm_disciplines(def);
    let reference = def.reference.enabled && !templates.is_empty();
    let control_seats = 0;
    let working = workers.len() + usize::from(reference);
    Starts {
        seats: control_seats + working,
        most_at_once: control_seats + working.max(templates.len()),
        control_seats,
        reference,
        waiting: waiting_tasks(def).into_iter().cloned().collect(),
        workers,
        templates,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Environment {
    pub repo_exists: bool,
    pub repo_worktree_of: Option<PathBuf>,
    pub agents: BTreeSet<String>,
    pub files: BTreeSet<String>,
    pub absent_files: BTreeSet<String>,
    pub open_surfaces: usize,
    pub reserved_surfaces: usize,
    pub max_surfaces: usize,
    pub backend_problem: Option<String>,
    pub push_problem: Option<String>,
}

pub const LINKED_WORKTREE_NOTE: &str = "the repo is a linked git worktree: it shares the main checkout's hooks through the common git dir, so no hook needs installing, and a seat there starts without the main checkout's untracked project files (.mcp.json, .env, .claude/settings.local.json) unless they are present in the worktree";

pub fn check_environment(def: &FleetDefinition, env: &Environment) -> Vec<Missing> {
    let mut found = Vec::new();
    if !def.repo.trim().is_empty() && !env.repo_exists {
        found.push(Missing::new(
            "repo",
            format!("`{}` is not a folder", def.repo),
        ));
    }
    if let Some(problem) = &env.push_problem {
        found.push(Missing::new(
            "repo",
            format!(
                "branch `{}` cannot be pushed from `{}`: {problem}",
                def.branch, def.repo
            ),
        ));
    }
    if env.repo_exists {
        found.extend(super::checks_files::unlisted_files(def, &env.absent_files));
    }
    for (item, agent) in agents_named(def) {
        if !agent.trim().is_empty() && !env.agents.contains(&agent) {
            found.push(Missing::new(item, format!("no agent file for `{agent}`")));
        }
    }
    for (item, file) in contracts_named(def) {
        if !env.files.contains(&file) {
            found.push(Missing::new(
                item,
                format!("`{file}` does not exist in the repo"),
            ));
        }
    }
    let wanted = starts(def).most_at_once;
    let taken = env.open_surfaces + env.reserved_surfaces;
    let room = env.max_surfaces.saturating_sub(taken);
    if env.max_surfaces > 0 && wanted > room {
        found.push(Missing::new("workers", no_room(wanted, room, env)));
    }
    if let Some(problem) = &env.backend_problem {
        found.push(Missing::new("terminal", problem.clone()));
    }
    found
}

fn no_room(wanted: usize, room: usize, env: &Environment) -> String {
    let reserved = match env.reserved_surfaces {
        0 => String::new(),
        seats => format!(", {seats} more being opened by a fleet that is starting"),
    };
    format!(
        "the fleet starts {wanted} seats and the terminal has room for {room} ({} open{reserved}, limit {})",
        env.open_surfaces, env.max_surfaces
    )
}

fn agents_named(def: &FleetDefinition) -> Vec<(String, String)> {
    let mut named = Vec::new();
    for (discipline, worker) in &def.workers {
        named.push((format!("workers.{discipline}.agent"), worker.agent.clone()));
    }
    named
}

fn contracts_named(def: &FleetDefinition) -> Vec<(String, String)> {
    let mut named = Vec::new();
    for (discipline, worker) in &def.workers {
        if let Some(file) = filled(&worker.contract) {
            named.push((format!("workers.{discipline}.contract"), file));
        }
    }
    named
}

fn filled(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

pub fn contract_path(repo: &str, file: &str) -> PathBuf {
    let path = Path::new(file);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(repo).join(path)
    }
}

pub async fn gather(
    def: &FleetDefinition,
    home: &Path,
    max_surfaces: usize,
    backend: &dyn TerminalBackend,
) -> Environment {
    let repo = Path::new(&def.repo);
    let state = match bundle::install(home) {
        Ok(state) => state,
        Err(problem) => {
            tracing::warn!(%problem, "mike could not install its fleet files; only the project's own are usable");
            bundle::default_state_dir(home)
        }
    };
    let agents = agents_named(def)
        .into_iter()
        .map(|(_, agent)| agent)
        .filter(|agent| {
            agent_file_exists(repo, home, agent) || bundle::generic_agent(agent).is_some()
        })
        .collect();
    let files = contracts_named(def)
        .into_iter()
        .map(|(_, file)| file)
        .filter(|file| bundle::resolve_contract(&def.repo, &state, file).is_some())
        .collect();
    let probe = backend.probe().await;
    let backend_problem = (!probe.available).then(|| {
        probe
            .checks
            .iter()
            .find(|c| !c.passed)
            .map(|c| format!("{}: {}", c.name, c.detail))
            .unwrap_or_else(|| format!("the {} backend is not available", probe.backend))
    });
    let open_surfaces = match backend.list().await {
        Ok(surfaces) => surfaces.len(),
        Err(_) => 0,
    };
    let repo_exists = !def.repo.trim().is_empty() && repo.is_dir();
    let push_problem = if repo_exists && !def.branch.trim().is_empty() {
        super::push::check_pushable(&def.repo, &def.branch)
            .await
            .err()
    } else {
        None
    };
    Environment {
        repo_exists,
        push_problem,
        repo_worktree_of: linked_worktree_root(repo).await,
        agents,
        files,
        absent_files: super::checks_files::absent_task_files(def, repo),
        open_surfaces,
        reserved_surfaces: 0,
        max_surfaces,
        backend_problem,
    }
}

pub async fn repo_is_linked_worktree(repo: &str) -> bool {
    linked_worktree_root(Path::new(repo)).await.is_some()
}

async fn linked_worktree_root(repo: &Path) -> Option<PathBuf> {
    match crate::launch::worktree::detect(repo).await {
        Ok(crate::launch::WorktreeKind::Linked { main_root }) => {
            Some(main_root.unwrap_or_else(|| PathBuf::from("the main checkout")))
        }
        _ => None,
    }
}

fn agent_file_exists(repo: &Path, home: &Path, agent: &str) -> bool {
    let name = agent.trim();
    if name.is_empty() || name.contains('/') {
        return false;
    }
    let file = format!("{name}.md");
    repo.join(".claude/agents").join(&file).is_file()
        || home.join(".claude/agents").join(&file).is_file()
}
