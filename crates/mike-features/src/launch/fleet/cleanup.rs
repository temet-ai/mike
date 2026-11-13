use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::definition::FleetDefinition;
use super::gate_slot::{check_slot_for, slot_for};
use super::plan::ControlSeats;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::{FleetCheckoutUser, FleetRow};
use crate::store::persistence::session_notes::NewSessionNote;

pub const RUN_CLONE_MARKER: &str = ".mike-run-clone";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Remove,
    Keep(String),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cleaned {
    pub removed: Vec<PathBuf>,
    pub kept: Option<String>,
}

async fn git_text(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| format!("git {} could not run: {e}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn marker_names(content: &str, slug: &str, run: &str) -> bool {
    content
        .lines()
        .map(str::trim)
        .any(|line| !line.is_empty() && (line == slug || line == run))
}

fn is_protected(repo: &Path) -> bool {
    if repo.components().count() < 3 {
        return true;
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    home.is_some_and(|home| home == repo)
}

fn dirty_lines<'a>(porcelain: &'a str, repo: &Path) -> Vec<&'a str> {
    porcelain
        .lines()
        .filter(|line| line.trim_end() != format!("?? {RUN_CLONE_MARKER}"))
        .filter(|line| !ignored_build_or_slot(line, repo))
        .filter(|line| !line.trim().is_empty())
        .collect()
}

fn ignored_build_or_slot(line: &str, repo: &Path) -> bool {
    let Some(path) = line.strip_prefix("!! ") else {
        return false;
    };
    match path.trim_end() {
        "target/" => true,
        ".claude/" => holds_only_slots(repo),
        path => path.starts_with(".claude/worktrees"),
    }
}

fn holds_only_slots(repo: &Path) -> bool {
    std::fs::read_dir(repo.join(".claude"))
        .map(|entries| entries.flatten().all(|e| e.file_name() == "worktrees"))
        .unwrap_or(false)
}

pub fn mark_run_clone(def: &FleetDefinition, project: &Path, slug: &str) {
    let repo = Path::new(&def.repo);
    let same = match (repo.canonicalize(), project.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => repo == project,
    };
    if same || !repo.join(".git").is_dir() {
        return;
    }
    let marker = repo.join(RUN_CLONE_MARKER);
    let known = std::fs::read_to_string(&marker).unwrap_or_default();
    if marker_names(&known, slug, slug) {
        return;
    }
    let written = std::fs::write(&marker, format!("{known}{slug}\n"));
    if let Err(err) = written {
        tracing::warn!(error = %err, repo = %def.repo, "fleet: the run's clone could not be marked");
    }
}

pub async fn verdict(repo: &Path, slug: &str, run: &str, in_use_by: Option<&str>) -> Verdict {
    if is_protected(repo) {
        return Verdict::Keep("the folder is not one a run could have made".to_string());
    }
    if let Some(other) = in_use_by {
        return Verdict::Keep(format!(
            "fleet {other} is not done and uses the same folder"
        ));
    }
    match std::fs::read_to_string(repo.join(RUN_CLONE_MARKER)) {
        Ok(content) if marker_names(&content, slug, run) => {}
        Ok(_) => return Verdict::Keep(format!("{RUN_CLONE_MARKER} names another run")),
        Err(_) => {
            return Verdict::Keep(format!(
                "no {RUN_CLONE_MARKER}: nothing says mike or a lead made it for this run"
            ));
        }
    }
    if !repo.join(".git").is_dir() {
        return Verdict::Keep("it is not a clone with its own .git folder".to_string());
    }
    unfinished_work(repo)
        .await
        .map_or(Verdict::Remove, Verdict::Keep)
}

pub fn other_user_of<'a>(
    repo: &str,
    own: Uuid,
    fleets: &'a [FleetCheckoutUser],
) -> Option<&'a str> {
    fleets
        .iter()
        .find(|fleet| {
            fleet.fleet_uuid != own && fleet.state != super::shutdown::DONE && fleet.repo == repo
        })
        .map(|fleet| fleet.slug.as_str())
}

async fn unfinished_work(repo: &Path) -> Option<String> {
    let status = match git_text(repo, &["status", "--porcelain", "--ignored"]).await {
        Ok(text) => text,
        Err(why) => return Some(why),
    };
    let dirty = dirty_lines(&status, repo);
    if !dirty.is_empty() {
        return Some(format!("{} uncommitted paths", dirty.len()));
    }
    let unpushed = git_text(
        repo,
        &[
            "rev-list",
            "--branches",
            "--not",
            "--remotes",
            "--max-count=1",
        ],
    )
    .await;
    match unpushed {
        Ok(text) if text.trim().is_empty() => {}
        Ok(_) => return Some("commits not pushed to any remote".to_string()),
        Err(why) => return Some(why),
    }
    match git_text(repo, &["stash", "list"]).await {
        Ok(text) if text.trim().is_empty() => None,
        Ok(_) => Some("a stash holds work".to_string()),
        Err(why) => Some(why),
    }
}

async fn remove_slot(repo: &Path, slot: &Path) -> bool {
    if slot == repo || slot.symlink_metadata().is_err() {
        return false;
    }
    let shown = slot.display().to_string();
    let _ = git_text(repo, &["worktree", "remove", "--force", &shown]).await;
    remove_tree(slot.to_path_buf()).await
}

async fn remove_tree(path: PathBuf) -> bool {
    let shown = path.display().to_string();
    let done = tokio::task::spawn_blocking(move || {
        if path.symlink_metadata().is_ok() {
            std::fs::remove_dir_all(&path)
        } else {
            Ok(())
        }
    })
    .await;
    let removed = matches!(done, Ok(Ok(())));
    if !removed {
        tracing::warn!(path = %shown, "fleet: a folder left behind by the closed run could not be removed; remove it by hand");
    }
    removed
}

pub async fn clean_run(repo: &Path, slug: &str, run: &str, in_use_by: Option<&str>) -> Cleaned {
    let mut cleaned = Cleaned::default();
    if let Verdict::Keep(why) = verdict(repo, slug, run, in_use_by).await {
        cleaned.kept = Some(why);
        return cleaned;
    }
    let repo_text = repo.to_string_lossy();
    let slots = [slot_for(&repo_text), check_slot_for(&repo_text)];
    for slot in slots.into_iter().flatten() {
        if remove_slot(repo, &slot).await {
            cleaned.removed.push(slot);
        }
    }
    if remove_tree(repo.to_path_buf()).await {
        cleaned.removed.push(repo.to_path_buf());
    }
    cleaned
}

pub fn offer_text(slug: &str, repo: &Path, why: &str) -> String {
    let text = repo.to_string_lossy();
    let folders: Vec<String> = [slot_for(&text), check_slot_for(&text)]
        .into_iter()
        .flatten()
        .map(|p| p.display().to_string())
        .chain(std::iter::once(repo.display().to_string()))
        .collect();
    format!(
        "[mike] Fleet {slug} is closed, but mike left its folders in place ({why}). They hold build output and can take tens of gigabytes. Once nothing in them is needed, remove them with: git -C {} worktree remove --force <the mike-gate and mike-check checkouts under .claude/worktrees>; rm -rf {}",
        repo.display(),
        folders.join(" ")
    )
}

async fn leave_operator_note(
    persist: &PersistHandle,
    row: &FleetRow,
    coordinator: Uuid,
    body: &str,
) {
    let title = format!(
        "Fleet {} left folders on disk: remove them when you are ready",
        row.slug
    );
    let tags = [format!("fleet:{}", row.fleet_uuid)];
    let note = NewSessionNote {
        kind: Some("note"),
        title: &title,
        body: Some(body),
        status: Some("operator"),
        tags: Some(&tags),
        data: None,
    };
    if let Err(err) = persist
        .launch()
        .create_session_note(coordinator, note)
        .await
    {
        tracing::warn!(error = %err, "fleet: could not leave the operator the note about the folders kept");
    }
}

async fn offer_kept(persist: &PersistHandle, row: &FleetRow, def: &FleetDefinition, why: &str) {
    tracing::warn!(fleet = %row.slug, repo = %def.repo, %why, "fleet: the closed run's folder was left in place");
    let Some(seats) = ControlSeats::recorded(def) else {
        return;
    };
    let body = offer_text(&row.slug, Path::new(&def.repo), why);
    leave_operator_note(persist, row, seats.coordinator, &body).await;
}

async fn record_seats_closed(persist: &PersistHandle, def: &FleetDefinition, run: &str) {
    use super::events::EventKind;
    let Ok(events) = persist.fleet().fleet_events_since(run, 0).await else {
        return;
    };
    let _ = def;
    let closed: std::collections::BTreeSet<Uuid> = events
        .iter()
        .filter(|e| e.kind == EventKind::SeatClosed)
        .filter_map(|e| e.session_uuid)
        .collect();
    for requested in events
        .iter()
        .filter(|e| e.kind == EventKind::SeatLaunchRequested)
    {
        let Some(session) = requested.session_uuid else {
            continue;
        };
        if closed.contains(&session) {
            continue;
        }
        let event = super::events::NewFleetEvent {
            run_id: run.to_string(),
            task: requested.task.clone(),
            session_uuid: Some(session),
            payload: super::events::EventPayload::SeatClosed(super::events::SeatClosed {
                reason: "the run closed".to_string(),
            }),
        };
        if let Err(err) = persist.fleet().append_fleet_event(&event).await {
            tracing::warn!(error = %err, run, "fleet: a seat close could not be recorded");
        }
    }
}

pub async fn after_close(
    persist: &PersistHandle,
    row: &FleetRow,
    def: &FleetDefinition,
    run: &str,
) {
    record_seats_closed(persist, def, run).await;
    let fleets = match persist.fleet().fleet_checkout_users().await {
        Ok(fleets) => fleets,
        Err(err) => {
            offer_kept(
                persist,
                row,
                def,
                &format!("cannot verify which fleets still need this checkout: {err}"),
            )
            .await;
            return;
        }
    };
    let other = other_user_of(&def.repo, row.fleet_uuid, &fleets);
    let cleaned = clean_run(Path::new(&def.repo), &row.slug, run, other).await;
    match &cleaned.kept {
        Some(why) => offer_kept(persist, row, def, why).await,
        None => {
            tracing::info!(fleet = %row.slug, removed = cleaned.removed.len(), "fleet: the closed run's clone and clean checkouts were removed");
        }
    }
}
