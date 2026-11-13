use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::collections::{BTreeSet, HashMap};

use uuid::Uuid;

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::definition_view::DefinitionError;
use super::tree::historical_run;
use crate::change_record::writers::change_record_enabled;
use crate::change_record::{ChangeEvent, ChangeKind, SessionIdentity};
use crate::launch::fleet::FleetDefinition;
use crate::launch::fleet::commit::owner_of;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::TreeChangeRow;

const AUTHORED: [ChangeKind; 2] = [ChangeKind::Edit, ChangeKind::ShellChange];
const AUTHOR_WINDOW_MS: i64 = 10_000;
const EVENTS_PER_ROW: usize = 4;
const MAX_EVENTS: i64 = 4000;
const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 1000;
const DEFAULT_WINDOW_HOURS: i64 = 6;

#[derive(Debug, Deserialize, Default)]
pub struct ChangesQuery {
    #[serde(default)]
    since: Option<DateTime<Utc>>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ChangeView {
    path: String,
    change: String,
    content_sha: Option<String>,
    seen_at: DateTime<Utc>,
    seat: Option<String>,
    task: Option<String>,
    owner_seat: Option<String>,
    attribution: &'static str,
    candidates: Vec<String>,
    note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChangesView {
    run_id: String,
    since: DateTime<Utc>,
    watching: bool,
    changes: Vec<ChangeView>,
}

pub async fn get_fleet_changes(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(query): Query<ChangesQuery>,
) -> Result<Json<ChangesView>, DefinitionError> {
    let (_, def, run) = historical_run(&state, &key).await?;
    let since = query
        .since
        .unwrap_or_else(|| Utc::now() - Duration::hours(DEFAULT_WINDOW_HOURS));
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let persist = state.persist().ok_or(DefinitionError::Unavailable)?;
    let rows = persist
        .fleet()
        .tree_changes_since(&run, since, query.path.as_deref(), limit)
        .await?;
    let authors = record_authors(persist, &def, &run, &rows).await;
    let changes = rows
        .into_iter()
        .map(|row| {
            let author = authors.get(&row.id).cloned().unwrap_or_default();
            change_view(&def, row, author)
        })
        .collect();
    Ok(Json(ChangesView {
        watching: crate::launch::fleet::watch::is_watching(&run),
        run_id: run,
        since,
        changes,
    }))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attribution<T> {
    seat: Option<T>,
    candidates: Vec<T>,
}

fn relevant_events<'a>(row: &TreeChangeRow, events: &'a [ChangeEvent]) -> Vec<&'a ChangeEvent> {
    let nearby: Vec<_> = events
        .iter()
        .filter(|e| AUTHORED.contains(&e.kind) && e.paths.contains(&row.path))
        .filter(|e| (e.at - row.seen_at).num_milliseconds().abs() <= AUTHOR_WINDOW_MS)
        .filter(|e| match (&row.content_sha, &e.after_sha) {
            (Some(row_sha), Some(event_sha)) => row_sha == event_sha,
            _ => true,
        })
        .collect();
    let exact: Vec<_> = nearby
        .iter()
        .copied()
        .filter(|e| row.content_sha.is_some() && e.after_sha == row.content_sha)
        .collect();
    if exact.is_empty() { nearby } else { exact }
}

pub fn author_of(row: &TreeChangeRow, events: &[ChangeEvent]) -> Attribution<i64> {
    let matching = relevant_events(row, events);
    let candidates: BTreeSet<i64> = matching
        .iter()
        .flat_map(|e| {
            e.session_id.into_iter().chain(
                e.detail["candidates"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(serde_json::Value::as_i64),
            )
        })
        .collect();
    let proven = !matching.is_empty()
        && row.content_sha.is_some()
        && matching.iter().all(|e| {
            e.after_sha == row.content_sha
                && e.session_id.is_some()
                && e.detail["content_verified"] == true
        });
    Attribution {
        seat: (proven && candidates.len() == 1)
            .then(|| candidates.first().copied())
            .flatten(),
        candidates: candidates.into_iter().collect(),
    }
}

fn seat_name(seats: &HashMap<Uuid, String>, identity: &SessionIdentity) -> String {
    seats
        .get(&identity.uuid)
        .cloned()
        .or_else(|| identity.name.clone())
        .unwrap_or_else(|| identity.uuid.to_string())
}

async fn record_authors(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    rows: &[TreeChangeRow],
) -> HashMap<i64, Attribution<String>> {
    if rows.is_empty() || !change_record_enabled(persist).await {
        return HashMap::new();
    }
    let mut paths: Vec<String> = rows.iter().map(|r| r.path.clone()).collect();
    paths.sort_unstable();
    paths.dedup();
    let limit = i64::try_from(rows.len() * EVENTS_PER_ROW)
        .unwrap_or(MAX_EVENTS)
        .clamp(1, MAX_EVENTS);
    let Ok(events) = persist
        .fleet()
        .latest_change_events(&def.repo, &paths, &AUTHORED, limit)
        .await
    else {
        return HashMap::new();
    };
    let found: Vec<_> = rows.iter().map(|r| (r.id, author_of(r, &events))).collect();
    let mut ids: Vec<i64> = found
        .iter()
        .flat_map(|(_, a)| a.candidates.clone())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let Ok(identities) = persist.session_identities(&ids).await else {
        return HashMap::new();
    };
    let seats: HashMap<Uuid, String> = persist
        .fleet()
        .fleet_seat_rows(run)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| Some((row.session_uuid?, row.seat?)))
        .collect();
    let names: HashMap<i64, String> = identities
        .iter()
        .map(|i| (i.id, seat_name(&seats, i)))
        .collect();
    found
        .into_iter()
        .map(|(row, author)| {
            let seat = author.seat.and_then(|id| names.get(&id).cloned());
            let candidates = author
                .candidates
                .into_iter()
                .map(|id| {
                    names
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| format!("session {id}"))
                })
                .collect();
            (row, Attribution { seat, candidates })
        })
        .collect()
}

pub fn change_view(
    def: &FleetDefinition,
    row: TreeChangeRow,
    author: Attribution<String>,
) -> ChangeView {
    let owner = owner_of(def, &row.path).map(|(task, _)| task.clone());
    let owner_seat = owner
        .as_ref()
        .and_then(|task| def.tasks.get(task))
        .and_then(|t| t.seat.clone());
    let seat = author.seat;
    let candidates: Vec<_> = author
        .candidates
        .into_iter()
        .chain(row.seats.unwrap_or_default())
        .chain(row.named.unwrap_or_default())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let attribution = if seat.is_some() { "proven" } else { "unknown" };
    let note = seat.is_none().then(|| {
        if candidates.is_empty() {
            "author unknown; task ownership is not authorship evidence".to_string()
        } else {
            format!("author unknown; inferred candidates (running commands or path mentions do not prove a write): {}", candidates.join(", "))
        }
    });
    let outside = match (&seat, &owner_seat) {
        (Some(s), Some(o)) if s != o => {
            Some(format!("written by {s}, but the path belongs to {o}"))
        }
        (Some(s), None) if owner.is_none() => {
            Some(format!("written by {s} outside every task's files"))
        }
        _ => None,
    };
    ChangeView {
        path: row.path,
        change: row.change,
        content_sha: row.content_sha,
        seen_at: row.seen_at,
        seat,
        task: owner,
        owner_seat,
        attribution,
        candidates,
        note: outside.or(note),
    }
}

#[cfg(test)]
#[path = "changes_tests.rs"]
mod tests;
