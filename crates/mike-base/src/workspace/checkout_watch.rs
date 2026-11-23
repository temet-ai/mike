use std::path::Path;

use uuid::Uuid;

use crate::store::persistence::location_lifecycle::{CheckoutMarks, CheckoutRow};
use crate::store::persistence::location_move::LocationMove;
use crate::store::persistence::{PersistError, PersistHandle};
use crate::workspace::git_repo::{RepoIdentity, off_request_threads, repo_identity};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Seen {
    present: bool,
    parent_dir_present: bool,
    identity: Option<RepoIdentity>,
    knowledge_project: Option<Uuid>,
}

impl Seen {
    fn of(row: &CheckoutRow) -> Self {
        let root = Path::new(row.root_path.trim_end_matches('/'));
        if !root.is_dir() {
            return Self {
                parent_dir_present: root.parent().is_some_and(Path::is_dir),
                ..Self::default()
            };
        }
        Self {
            present: true,
            parent_dir_present: true,
            identity: repo_identity(root).filter(|identity| identity.checkout_root == root),
            knowledge_project: knowledge_project(root),
        }
    }

    fn is_main_checkout(&self) -> bool {
        self.identity
            .as_ref()
            .is_some_and(|identity| identity.main_root == identity.checkout_root)
    }
}

fn knowledge_project(root: &Path) -> Option<Uuid> {
    let bytes = std::fs::read(root.join(".mike").join("knowledge.json")).ok()?;
    let config: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    config.get("project_uuid")?.as_str()?.parse().ok()
}

pub async fn watch_checkouts(
    persist: &PersistHandle,
    machine: Uuid,
) -> Result<Vec<LocationMove>, PersistError> {
    let rows = persist.checkout_rows(machine).await?;
    let Some(seen) = look(&rows).await else {
        return Ok(Vec::new());
    };
    persist
        .record_checkout_marks(machine, &marks(&rows, &seen))
        .await?;
    if !any_vanished_main(&rows, &seen) {
        return Ok(Vec::new());
    }
    let rows = persist.read_checkout_rows(machine).await?;
    let Some(seen) = look(&rows).await else {
        return Ok(Vec::new());
    };
    let mut moves = Vec::new();
    for (location, root) in automatic_moves(&rows, &seen) {
        moves.push(persist.move_location(location, &root).await?);
    }
    Ok(moves)
}

async fn look(rows: &[CheckoutRow]) -> Option<Vec<Seen>> {
    let reading = rows.to_vec();
    off_request_threads("checkout presence", move || {
        reading.iter().map(Seen::of).collect::<Vec<Seen>>()
    })
    .await
}

fn any_vanished_main(rows: &[CheckoutRow], seen: &[Seen]) -> bool {
    rows.iter()
        .zip(seen)
        .any(|(row, seen)| vanished_main(row, seen))
}

pub(super) fn marks(rows: &[CheckoutRow], seen: &[Seen]) -> CheckoutMarks {
    let mut marks = CheckoutMarks::default();
    for (row, seen) in rows.iter().zip(seen) {
        marks.presence(row.location_uuid, seen.present);
        if let Some(identity) = &seen.identity {
            let main_root = (identity.main_root != identity.checkout_root)
                .then(|| identity.main_root.to_string_lossy().into_owned());
            if parent_in(rows, row, main_root.as_deref()) != row.parent_location_uuid {
                marks.classify(row.location_uuid, main_root);
            }
        }
    }
    marks
}

fn parent_in(rows: &[CheckoutRow], row: &CheckoutRow, main_root: Option<&str>) -> Option<Uuid> {
    let main_root = main_root?;
    rows.iter()
        .find(|other| other.root_path.trim_end_matches('/') == main_root)
        .map(|other| other.location_uuid)
        .filter(|parent| *parent != row.location_uuid)
}

pub(super) fn automatic_moves(rows: &[CheckoutRow], seen: &[Seen]) -> Vec<(Uuid, String)> {
    let pairs: Vec<(usize, usize)> = rows
        .iter()
        .zip(seen)
        .enumerate()
        .filter(|(_, (row, seen))| vanished_main(row, seen))
        .filter_map(|(old, _)| sole_candidate(rows, seen, old).map(|new| (old, new)))
        .collect();
    pairs
        .iter()
        .filter(|(old, new)| {
            pairs.iter().filter(|(_, other)| other == new).count() == 1
                && !pairs
                    .iter()
                    .any(|(outer, _)| outer != old && contains(&rows[*outer], &rows[*old]))
        })
        .map(|(old, new)| {
            let root = rows[*new].root_path.trim_end_matches('/').to_owned();
            (rows[*old].location_uuid, root)
        })
        .collect()
}

fn vanished_main(row: &CheckoutRow, seen: &Seen) -> bool {
    !seen.present
        && seen.parent_dir_present
        && row.parent_location_uuid.is_none()
        && row.present_at.is_some()
}

fn sole_candidate(rows: &[CheckoutRow], seen: &[Seen], old: usize) -> Option<usize> {
    let mut found = rows
        .iter()
        .zip(seen)
        .enumerate()
        .filter(|(index, (row, seen))| *index != old && same_checkout(&rows[old], row, seen))
        .map(|(index, _)| index);
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

fn same_checkout(old: &CheckoutRow, row: &CheckoutRow, seen: &Seen) -> bool {
    let registered_after = old
        .present_at
        .is_some_and(|present_at| row.created_at > present_at);
    let same_origin = old.origin_url.is_some()
        && seen
            .identity
            .as_ref()
            .is_some_and(|identity| identity.origin_url == old.origin_url);
    let same_declaration = seen.knowledge_project == Some(old.project_uuid);
    seen.present
        && seen.is_main_checkout()
        && row.project_id == old.project_id
        && registered_after
        && (same_origin || same_declaration)
}

fn contains(outer: &CheckoutRow, inner: &CheckoutRow) -> bool {
    let outer = outer.root_path.trim_end_matches('/');
    inner.root_path.starts_with(&format!("{outer}/"))
}

#[cfg(test)]
#[path = "checkout_watch_db_tests.rs"]
mod db_tests;

#[cfg(test)]
#[path = "checkout_watch_tests.rs"]
mod tests;
