use std::collections::BTreeMap;

use super::commit::covers;
use super::definition::{CheckDefinition, FleetDefinition};
use super::diagnostics::Diagnostic;
use super::gate::Target;
use super::gate_slot::check_slot_for;
use super::watch::git;

const STATE_SHOWN: usize = 12;
const COMMIT_SHOWN: usize = 10;

pub fn parse_tree(raw: &[u8]) -> BTreeMap<String, String> {
    parse_entries(raw, false)
}

pub(super) fn state_tree(raw: &[u8]) -> BTreeMap<String, String> {
    parse_entries(raw, true)
}

fn parse_entries(raw: &[u8], metadata: bool) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    for entry in raw.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let text = String::from_utf8_lossy(entry);
        let Some((meta, path)) = text.split_once('\t') else {
            continue;
        };
        if let Some(sha) = meta.split_whitespace().nth(2) {
            files.insert(
                path.to_string(),
                if metadata { meta } else { sha }.to_string(),
            );
        }
    }
    files
}

pub async fn state_at(repo: &str, check: &CheckDefinition, rev: &str) -> Result<String, String> {
    let raw = git(repo, &["ls-tree", "-r", "-z", rev], None)
        .await
        .ok_or_else(|| {
            format!(
                "{} could not be read from the repository",
                short_commit(rev)
            )
        })?;
    state_of(&state_tree(&raw), &check.covered()).map(|state| command_state(&state, &check.command)).ok_or_else(|| {
        format!(
            "the check's paths ({}) match no committed file at {}, so no state can be named for it; fix `check.paths` in the definition",
            check.paths.join(", "),
            short_commit(rev)
        )
    })
}

pub async fn state_for(def: &FleetDefinition, rev: &str) -> Result<String, String> {
    let mut check = def.check.clone();
    let operation = def.execution.operation(&check.command);
    if let Some(op) = operation.filter(|op| !op.inputs.is_empty()) {
        check.paths.clone_from(&op.inputs);
    }
    let state = state_at(&def.repo, &check, rev).await?;
    let policy = serde_json::to_string(&(def.execution.environment_policy(), operation))
        .map_err(|error| error.to_string())?;
    let state = command_state(&state, &policy);
    Ok(if super::operation::reusable(def) {
        state
    } else {
        let commit = git(
            &def.repo,
            &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
            None,
        )
        .await
        .ok_or_else(|| format!("cannot resolve source revision {rev}"))?;
        command_state(&state, &String::from_utf8_lossy(&commit))
    })
}

pub(super) fn command_state(state: &str, command: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"mike-check-isolated-source-v2\0");
    hasher.update(command.as_bytes());
    hasher.update(b"\0");
    hasher.update(state.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn state_of(known: &BTreeMap<String, String>, paths: &[String]) -> Option<String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut files = 0usize;
    for (path, sha) in known {
        if paths.is_empty() || covers(paths, path) {
            hasher.update(path.as_bytes());
            hasher.update(b"\0");
            hasher.update(sha.as_bytes());
            hasher.update(b"\n");
            files += 1;
        }
    }
    (files > 0).then(|| {
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    })
}

pub fn short(state: &str) -> &str {
    &state[..state.len().min(STATE_SHOWN)]
}

pub fn short_commit(commit: &str) -> &str {
    &commit[..commit.len().min(COMMIT_SHOWN)]
}

pub fn found_in(def: &FleetDefinition, output: &str) -> Vec<Diagnostic> {
    super::diagnostics::parse(output, &def.repo)
}

pub(super) async fn head(def: &FleetDefinition) -> Result<Target, String> {
    let reference = if def.branch.trim().is_empty() {
        "HEAD".to_string()
    } else {
        format!("refs/heads/{}", def.branch.trim())
    };
    let commit = git(
        &def.repo,
        &["rev-parse", "--verify", "-q", &reference],
        None,
    )
    .await
    .map(|raw| String::from_utf8_lossy(&raw).trim().to_string())
    .filter(|commit| !commit.is_empty())
    .ok_or_else(|| format!("the branch `{}` has no commit to check", def.branch))?;
    let slot = check_slot_for(&def.repo)
        .ok_or_else(|| format!("`{}` is not a checkout to run the check in", def.repo))?;
    Ok(Target {
        commit,
        pushed: false,
        slot,
    })
}
