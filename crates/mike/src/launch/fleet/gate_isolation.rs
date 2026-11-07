use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::launch::fleet::gate::Target;

use super::{GATE_TIMEOUT, OUTPUT_FILE, managed_directory, prepare, run_check, run_gate_logged};

pub(in crate::launch::fleet) enum Operation {
    Check { timeout: Duration, configured: bool },
    Gate { completion_log: Option<PathBuf> },
}

impl Operation {
    fn timeout(&self) -> Duration {
        match self {
            Self::Check { timeout, .. } => *timeout,
            Self::Gate { .. } => GATE_TIMEOUT,
        }
    }

    async fn run(&self, at: &Target, command: &str, run: &str) -> (Option<i32>, String) {
        match self {
            Self::Check {
                timeout,
                configured,
            } => run_check(&at.slot, command, run, (*timeout, "the check"), *configured).await,
            Self::Gate { .. } => {
                run_gate_logged(&at.slot, command, run, &at.slot.join(OUTPUT_FILE)).await
            }
        }
    }
}

impl Operation {
    async fn publish(&self, at: &Target, run: &str, output: &str) {
        let Self::Gate { completion_log } = self else {
            return;
        };
        if let Err(why) = managed_directory(&at.slot) {
            tracing::warn!(%run, %why, "fleet: gate log directory is no longer safe");
            return;
        }
        let latest = at.slot.join(OUTPUT_FILE);
        for log in std::iter::once(&latest).chain(completion_log.iter()) {
            write_log(run, log, output).await;
        }
    }
}

async fn write_log(run: &str, path: &Path, output: &str) {
    if let Err(err) = tokio::fs::write(path, output).await {
        tracing::warn!(%run, %err, path = %path.display(), "fleet: could not save the gate's final isolated log");
    }
}

const WORKTREE_ADD_LOCK: &str = "worktree-add";
const WORKTREE_ADD_WAIT: Duration = Duration::from_secs(60);

pub(in crate::launch::fleet) async fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    crate::launch::fleet::owned_git::run(repo, args).await
}

async fn lock(repo: &str, names: &[String]) -> Result<(File, String), String> {
    let directory = managed_directory(Path::new(repo))?;
    let mut files = Vec::new();
    for name in names {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(directory.join(format!("{name}.lock")))
            .map_err(|e| e.to_string())?;
        files.push((file, name.clone()));
    }
    loop {
        for index in 0..files.len() {
            match files[index].0.try_lock() {
                Ok(()) => return Ok(files.swap_remove(index)),
                Err(std::fs::TryLockError::WouldBlock) => {}
                Err(std::fs::TryLockError::Error(e)) => return Err(e.to_string()),
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn candidates(operation: &Operation, at: &Target) -> Vec<String> {
    match operation {
        Operation::Check { .. } => super::CHECK_SLOT_NAMES
            .iter()
            .map(|name| (*name).to_string())
            .collect(),
        Operation::Gate { .. } => at
            .slot
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .into_iter()
            .collect(),
    }
}

fn in_slot(at: &Target, name: &str) -> Target {
    let slot = at
        .slot
        .parent()
        .map_or_else(|| at.slot.clone(), |parent| parent.join(name));
    Target { slot, ..at.clone() }
}

fn same_paths(output: &str, from: &Path, to: &Path) -> String {
    let canonical =
        |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    output.replace(
        &canonical(from).to_string_lossy().into_owned(),
        &canonical(to).to_string_lossy(),
    )
}

async fn clean(slot: &Path) -> Result<(), String> {
    if !git(slot, &["status", "--porcelain", "--untracked-files=normal"])
        .await?
        .is_empty()
    {
        return Err(refusal(slot));
    }
    Ok(())
}

async fn reset(slot: &Path, commit: &str) -> Result<Option<String>, String> {
    let changed = git(slot, &["status", "--porcelain", "--untracked-files=normal"]).await?;
    if changed.is_empty() {
        return Ok(None);
    }
    if changed.lines().any(|line| line.starts_with("??")) {
        return Err(refusal(slot));
    }
    git(slot, &["checkout", "--force", "--detach", commit]).await?;
    Ok(Some(format!(
        "\nBefore this run, mike put back tracked changes that an earlier run left in its checkout, so the run starts from the checked commit. Changed:\n{changed}\n"
    )))
}

fn refusal(slot: &Path) -> String {
    format!(
        "isolated checkout `{}` has changes; inspect them before retrying; no files were reset, cleaned or stashed",
        slot.display()
    )
}

async fn add_slot(repo: &str, slot: &Path, commit: &str) -> Result<(), String> {
    let names = [WORKTREE_ADD_LOCK.to_string()];
    let _adding = tokio::time::timeout(WORKTREE_ADD_WAIT, lock(repo, &names))
        .await
        .map_err(|_| "timed out waiting to add the isolated checkout".to_string())??;
    git(Path::new(repo), &["worktree", "list", "--porcelain"]).await?;
    let name = slot.to_string_lossy();
    git(
        Path::new(repo),
        &["worktree", "add", "--detach", &name, commit],
    )
    .await?;
    Ok(())
}

async fn checkout(repo: &str, at: &Target) -> Result<Option<String>, String> {
    crate::launch::fleet::managed_command::preparation_allowed()?;
    let slot = &at.slot;
    let name = slot
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| super::managed_slot_name(name))
        .ok_or("isolated path is not a managed fleet worktree")?;
    let expected = std::fs::canonicalize(Path::new(repo))
        .map_err(|e| e.to_string())?
        .join(".claude/worktrees")
        .join(name);
    let parent = slot
        .parent()
        .ok_or("isolated checkout has no parent directory")?;
    if std::fs::canonicalize(parent).map_err(|e| e.to_string())?
        != expected
            .parent()
            .ok_or("isolated checkout has no expected parent")?
    {
        return Err(
            "isolated directory resolves outside this checkout; no checkout was changed"
                .to_string(),
        );
    }
    if !slot.exists() {
        add_slot(repo, slot, &at.commit).await?;
    }
    let actual = std::fs::canonicalize(slot).map_err(|e| e.to_string())?;
    if actual != expected || !slot.join(".git").is_file() {
        return Err(
            "isolated path is not mike's isolated worktree; no checkout was changed".to_string(),
        );
    }
    let top = git(slot, &["rev-parse", "--show-toplevel"]).await?;
    if std::fs::canonicalize(top).map_err(|e| e.to_string())? != actual {
        return Err(
            "isolated git directory points at another checkout; no checkout was changed"
                .to_string(),
        );
    }
    let args = ["rev-parse", "--path-format=absolute", "--git-common-dir"];
    let common = git(Path::new(repo), &args).await?;
    let slot_common = git(slot, &args).await?;
    if std::fs::canonicalize(common).map_err(|e| e.to_string())?
        != std::fs::canonicalize(slot_common).map_err(|e| e.to_string())?
        || git(slot, &["rev-parse", "--abbrev-ref", "HEAD"]).await? != "HEAD"
    {
        return Err(
            "isolated path is not a detached worktree of this repository; no checkout was changed"
                .to_string(),
        );
    }
    let restored = reset(slot, &at.commit).await?;
    if git(slot, &["rev-parse", "HEAD"]).await? != at.commit {
        git(
            slot,
            &["checkout", "--no-overwrite-ignore", "--detach", &at.commit],
        )
        .await?;
    }
    clean(slot).await?;
    if git(slot, &["rev-parse", "HEAD"]).await? != at.commit {
        return Err("isolated checkout did not remain at the requested commit".to_string());
    }
    Ok(restored)
}

fn decorate(
    output: &str,
    at: &Target,
    original: &Target,
    restored: Option<String>,
    code: Option<i32>,
) -> String {
    let mut output = same_paths(output, &at.slot, &original.slot);
    if let Some(note) = restored {
        output = format!("{note}{output}");
    }
    if code != Some(0) {
        output.push_str(&format!("\nExecution checkout: {}. Managed checks do not inherit shared untracked dependencies or checkout-hook setup; prepare required ignored dependencies and setup in the configured command. This slot retains its own build cache for reuse.\n", at.slot.display()));
    }
    output
}

async fn verified_result(
    at: &Target,
    code: Option<i32>,
    output: String,
) -> (Option<i32>, String, bool) {
    match crate::launch::fleet::gate_source::verify(at).await {
        Ok(()) => (code, output, true),
        Err(why) => (
            None,
            format!(
                "{output}\nIsolated result is not valid; source unverified after execution: {why}\nOriginal command exit: {code:?}; this output cannot prove commit {}.",
                at.commit
            ),
            false,
        ),
    }
}

pub(in crate::launch::fleet) async fn execute(
    repo: &str,
    at: &Target,
    run: &str,
    command: &str,
    operation: Operation,
) -> (Option<i32>, String, bool) {
    let context = match crate::launch::fleet::managed_command::current() {
        Ok(context) => context,
        Err(why) => return (None, why, false),
    };
    let names = candidates(&operation, at);
    let held = tokio::select! {
        biased;
        _ = context.owner.token().cancelled() => return (None, crate::launch::fleet::managed_command::cancelled(&context.owner), false),
        held = tokio::time::timeout(operation.timeout(), lock(repo, &names)) => held,
    };
    let (_held, name) = match held {
        Ok(Ok(held)) => held,
        Ok(Err(why)) => return (None, why, false),
        Err(_) => {
            return (
                None,
                "timed out waiting for the isolated checkout".into(),
                false,
            );
        }
    };
    let _held = context.resources.retain(_held);
    let original = at;
    let chosen = in_slot(at, &name);
    let at = &chosen;
    let restored = match prepare_execution(repo, at).await {
        Ok(restored) => restored,
        Err(why) => return (None, why, false),
    };
    let (code, output) = operation.run(at, command, run).await;
    let output = decorate(&output, at, original, restored, code);
    if context.owner.token().is_cancelled() {
        operation.publish(at, run, &output).await;
        return (None, output, false);
    }
    let mut result = verified_result(at, code, output).await;
    if let Some(note) = restore(at).await {
        result.1.push_str(&note);
    }
    operation.publish(at, run, &result.1).await;
    result
}

async fn restore(at: &Target) -> Option<String> {
    let changed = git(
        &at.slot,
        &["status", "--porcelain", "--untracked-files=normal"],
    )
    .await
    .ok()
    .filter(|status| !status.is_empty())?;
    let reset = async {
        git(&at.slot, &["checkout", "--force", "--detach", &at.commit]).await?;
        git(&at.slot, &["clean", "-fd"]).await
    }
    .await;
    Some(match reset {
        Ok(_) => format!(
            "\nThe command changed files in mike's clean checkout; they were put back so later checks can run. A check must not write source files (use a check-only form). Changed:\n{changed}\n"
        ),
        Err(why) => format!(
            "\nThe command changed files in mike's clean checkout and mike could not put them back ({why}). Changed:\n{changed}\n"
        ),
    })
}

async fn prepare_execution(repo: &str, at: &Target) -> Result<Option<String>, String> {
    prepare(repo).await?;
    let restored = checkout(repo, at).await?;
    prepare(&at.slot.to_string_lossy()).await?;
    crate::launch::fleet::managed_command::preparation_allowed()?;
    Ok(restored)
}
