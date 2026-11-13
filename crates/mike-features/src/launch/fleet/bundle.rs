use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const WORKER_SKILL: &str = include_str!("../../../../../.claude/skills/fleet-worker/SKILL.md");

const SKILLS: [(&str, &str); 1] = [("fleet-worker", WORKER_SKILL)];

const AGENTS: [(&str, &str); 2] = [
    (
        "fleet-rule-auditor",
        include_str!("../../../../../.claude/agents/fleet-rule-auditor.md"),
    ),
    (
        "fleet-capability-author",
        include_str!("../../../../../.claude/agents/fleet-capability-author.md"),
    ),
];

const FILES: [(&str, &str); 3] = [
    (
        "rules.md",
        include_str!("../../../../../.claude/fleet/rules.md"),
    ),
    ("worker-contract.md", WORKER_SKILL),
    (
        "outcome-checks.sh",
        include_str!("../../../../../.claude/fleet/outcome-checks.sh"),
    ),
];

pub const COMMAND: &str = include_str!("../../../../../.claude/fleet/fleet-command.md");
pub const PROJECT_FILE: &str = "project.md";
const FLEET_DIR: &str = ".claude/fleet";
const AGENTS_DIR: &str = ".claude/agents";
const SKILLS_DIR: &str = ".claude/skills";
const COMMANDS_DIR: &str = ".claude/commands";
const INSTALLED_LEDGER: &str = ".mike-installed.json";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FleetPaths {
    pub rules: String,
    pub checks: String,
    pub project: Option<String>,
}

pub fn home() -> PathBuf {
    crate::config::paths::live_install_guard::home().unwrap_or_else(|| PathBuf::from("/"))
}

pub fn paths_for(repo: &str) -> FleetPaths {
    let state = default_state_dir(&home());
    let text = |name: &str| {
        resolve_file(repo, &state, name)
            .unwrap_or_else(|| Path::new(repo).join(FLEET_DIR).join(name))
            .to_string_lossy()
            .into_owned()
    };
    FleetPaths {
        rules: text("rules.md"),
        checks: text("outcome-checks.sh"),
        project: project_file(repo, PROJECT_FILE).map(|p| p.to_string_lossy().into_owned()),
    }
}

pub fn generic_agent(name: &str) -> Option<&'static str> {
    AGENTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, text)| *text)
}

pub fn generic_file(name: &str) -> Option<&'static str> {
    FILES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, text)| *text)
}

pub fn state_dir(xdg_state_home: Option<&str>, home: &Path) -> PathBuf {
    match xdg_state_home.filter(|v| !v.is_empty()) {
        Some(xdg) => PathBuf::from(xdg),
        None => home.join(".local").join("state"),
    }
    .join("mike")
    .join("fleet")
}

pub fn default_state_dir(home: &Path) -> PathBuf {
    let dir = state_dir(crate::config::env_text("XDG_STATE_HOME").as_deref(), home);
    crate::config::paths::live_install_guard::refuse(&dir);
    dir
}

pub fn project_file(repo: &str, name: &str) -> Option<PathBuf> {
    let path = Path::new(repo).join(FLEET_DIR).join(name);
    path.is_file().then_some(path)
}

pub fn resolve_file(repo: &str, state: &Path, name: &str) -> Option<PathBuf> {
    if let Some(own) = project_file(repo, name) {
        return Some(own);
    }
    let generic = state.join(name);
    (generic_file(name).is_some() && generic.is_file()).then_some(generic)
}

pub fn resolve_contract(repo: &str, state: &Path, file: &str) -> Option<PathBuf> {
    let path = Path::new(file);
    if path.is_absolute() {
        return path.is_file().then(|| path.to_path_buf());
    }
    let in_repo = Path::new(repo).join(path);
    if in_repo.is_file() {
        return Some(in_repo);
    }
    let name = path.file_name()?.to_str()?;
    resolve_file(repo, state, name)
}

pub fn install_files(state: &Path) -> Result<(), String> {
    std::fs::create_dir_all(state)
        .map_err(|e| format!("could not create {}: {e}", state.display()))?;
    for (name, text) in FILES {
        let path = state.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
        }
        if std::fs::read_to_string(&path).ok().as_deref() != Some(text) {
            std::fs::write(&path, text)
                .map_err(|e| format!("could not write {}: {e}", path.display()))?;
        }
    }
    let script = state.join("outcome-checks.sh");
    let mut mode = std::fs::metadata(&script)
        .map_err(|e| format!("could not read {}: {e}", script.display()))?
        .permissions();
    use std::os::unix::fs::PermissionsExt;
    mode.set_mode(0o755);
    std::fs::set_permissions(&script, mode)
        .map_err(|e| format!("could not mark {} executable: {e}", script.display()))?;
    Ok(())
}

fn sha(text: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_ledger(dir: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(dir.join(INSTALLED_LEDGER))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_ledger(dir: &Path, ledger: &BTreeMap<String, String>) -> Result<(), String> {
    let text = serde_json::to_string_pretty(ledger).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(INSTALLED_LEDGER), text).map_err(|e| {
        format!(
            "could not write the installed-files ledger in {}: {e}",
            dir.display()
        )
    })
}

pub fn install_agents(home: &Path) -> Result<Vec<String>, String> {
    install_owned(&home.join(AGENTS_DIR), &AGENTS, |name| format!("{name}.md"))
}

pub fn install_skills(home: &Path) -> Result<Vec<String>, String> {
    install_owned(&home.join(SKILLS_DIR), &SKILLS, |name| {
        format!("{name}/SKILL.md")
    })
}

fn install_owned(
    dir: &Path,
    files: &[(&str, &str)],
    file_for: fn(&str) -> String,
) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let mut ledger = read_ledger(dir);
    let mut written = Vec::new();
    for &(name, text) in files {
        let path = dir.join(file_for(name));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
        }
        let current = std::fs::read_to_string(&path).ok();
        let mike_owns = match &current {
            None => true,
            Some(existing) => ledger
                .get(name)
                .is_some_and(|recorded| *recorded == sha(existing)),
        };
        if !mike_owns || current.as_deref() == Some(text) {
            continue;
        }
        std::fs::write(&path, text)
            .map_err(|e| format!("could not write {}: {e}", path.display()))?;
        ledger.insert(name.to_string(), sha(text));
        written.push(name.to_string());
    }
    if !written.is_empty() {
        write_ledger(dir, &ledger)?;
    }
    Ok(written)
}

pub fn install_command(home: &Path) -> Result<bool, String> {
    let dir = home.join(COMMANDS_DIR);
    let path = dir.join("fleet.md");
    if path.symlink_metadata().is_ok() {
        return Ok(false);
    }
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    std::fs::write(&path, COMMAND)
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(true)
}

pub fn install(home: &Path) -> Result<PathBuf, String> {
    let state = default_state_dir(home);
    install_files(&state)?;
    let command = install_command(home)?;
    let mut agents = install_agents(home)?;
    agents.extend(install_skills(home)?);
    report_installed(command, &agents);
    Ok(state)
}

fn report_installed(command: bool, agents: &[String]) {
    if command {
        note_command();
    }
    agents.iter().for_each(|name| note_agent(name));
}

fn note_command() {
    tracing::info!("mike installed the /fleet command at user level");
}

fn note_agent(name: &str) {
    tracing::info!(file = %name, "mike installed its fleet agent or skill at user level; a project's own copy takes precedence");
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn a_projects_own_file_wins_over_the_generic_one() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let repo = dir.path().join("repo");
        let state = dir.path().join("state");
        std::fs::create_dir_all(repo.join(".claude/fleet"))?;
        install_files(&state).map_err(anyhow::Error::msg)?;
        let repo_text = repo.to_string_lossy().into_owned();
        assert_eq!(
            resolve_file(&repo_text, &state, "rules.md"),
            Some(state.join("rules.md"))
        );
        std::fs::write(repo.join(".claude/fleet/rules.md"), "our own rules")?;
        assert_eq!(
            resolve_file(&repo_text, &state, "rules.md"),
            Some(repo.join(".claude/fleet/rules.md"))
        );
        assert_eq!(resolve_file(&repo_text, &state, "nothing.md"), None);
        assert_eq!(
            resolve_contract(&repo_text, &state, ".claude/fleet/worker-contract.md"),
            Some(state.join("worker-contract.md")),
            "a contract named by its repository path falls back to mike's copy"
        );
        Ok(())
    }

    #[test]
    fn installed_files_track_the_binary_and_the_script_is_executable() -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir()?;
        let state = dir.path().join("state");
        install_files(&state).map_err(anyhow::Error::msg)?;
        std::fs::write(state.join("rules.md"), "stale")?;
        install_files(&state).map_err(anyhow::Error::msg)?;
        assert_eq!(
            std::fs::read_to_string(state.join("rules.md"))?,
            generic_file("rules.md").unwrap_or_default()
        );
        let mode = std::fs::metadata(state.join("outcome-checks.sh"))?
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111);
        Ok(())
    }

    #[test]
    fn the_fleet_command_is_written_once_and_a_link_or_edit_is_left_alone() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let home = dir.path();
        assert!(install_command(home).map_err(anyhow::Error::msg)?);
        let path = home.join(".claude/commands/fleet.md");
        assert_eq!(std::fs::read_to_string(&path)?, COMMAND);
        std::fs::write(&path, "edited")?;
        assert!(!install_command(home).map_err(anyhow::Error::msg)?);
        assert_eq!(std::fs::read_to_string(&path)?, "edited");
        Ok(())
    }

    #[test]
    fn the_worker_is_a_skill_any_agent_loads_and_no_longer_an_agent() -> anyhow::Result<()> {
        assert!(generic_agent("fleet-worker").is_none());
        assert!(generic_agent("rust-developer").is_none());
        let dir = tempfile::tempdir()?;
        let home = dir.path();
        assert_eq!(
            install_skills(home).map_err(anyhow::Error::msg)?,
            vec!["fleet-worker".to_string()]
        );
        let skill = std::fs::read_to_string(home.join(".claude/skills/fleet-worker/SKILL.md"))?;
        assert!(skill.contains("name: fleet-worker"));
        assert_eq!(generic_file("worker-contract.md"), Some(skill.as_str()));
        Ok(())
    }

    #[test]
    fn an_agent_file_the_operator_edited_is_never_overwritten() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let home = dir.path();
        let first = install_agents(home).map_err(anyhow::Error::msg)?;
        assert_eq!(first.len(), AGENTS.len());
        assert!(first.iter().any(|name| name == "fleet-capability-author"));
        let path = home.join(".claude/agents/fleet-rule-auditor.md");
        assert_eq!(
            install_agents(home).map_err(anyhow::Error::msg)?,
            Vec::<String>::new()
        );
        std::fs::write(&path, "my own auditor")?;
        assert_eq!(
            install_agents(home).map_err(anyhow::Error::msg)?,
            Vec::<String>::new()
        );
        assert_eq!(std::fs::read_to_string(&path)?, "my own auditor");
        Ok(())
    }

    #[test]
    fn neither_the_leaf_prompts_nor_a_control_seat_role_file_is_written() -> anyhow::Result<()> {
        for control in ["fleet-coordinator", "fleet-acceptance", "fleet-observer"] {
            assert!(generic_agent(control).is_none(), "{control}");
        }
        assert!(generic_file("observer-contract.md").is_none());
        let dir = tempfile::tempdir()?;
        let state = dir.path().join("state");
        install_files(&state).map_err(anyhow::Error::msg)?;
        assert!(state.join("rules.md").is_file());
        assert!(!state.join("leaves").exists());
        Ok(())
    }
}
