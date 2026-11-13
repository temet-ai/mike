use crate::store::persistence::fleet_repo::FleetPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::definition_view::DefinitionError;
use crate::api::changes::{WithTail, since_you_last_looked};
use crate::launch::fleet::commit::{
    CommitReport, CommitRequest, IntegrateReport, commit, covers, integrate,
};
use crate::launch::fleet::commit_note;
use crate::launch::fleet::commit_verify::Verifier;
use crate::launch::fleet::reuse::waiting_uses;
use crate::launch::fleet::seated::Seated;
use crate::launch::fleet::{FleetDefinition, checks::normal};
use crate::state::AppState;
use crate::store::persistence::fleet::{FleetRow, TreeResultRow};
use crate::store::persistence::{PersistError, PersistHandle};

const DEFAULT_WINDOW_HOURS: i64 = 6;
const OPEN_READ: i64 = 200;
const TAIL_LINES: usize = 30;
const CHANGED_SHOWN: usize = 20;
const RESULT_VALIDITY_PROVENANCE: &str = "Raw Bash capture has no commit binding, dependency closure or complete change-coverage proof. still_valid null means unproven; false means observed invalidation or update mode. Any checkout change since command start invalidates, even outside paths; paths only filters changed_since. Tool success is not a test pass.";

pub(crate) async fn historical_run(
    state: &AppState,
    key: &str,
) -> Result<(FleetRow, FleetDefinition, String), DefinitionError> {
    let persist = state.persist().ok_or(DefinitionError::Unavailable)?;
    let row = persist
        .fleet()
        .find_fleet(key)
        .await?
        .ok_or_else(|| DefinitionError::NotFound(key.to_string()))?;
    let def =
        FleetDefinition::from_value(&row.definition).map_err(DefinitionError::InvalidDefinition)?;
    let run = row
        .run_id
        .clone()
        .ok_or_else(|| DefinitionError::NotRunning {
            slug: row.slug.clone(),
            state: row.state.clone(),
        })?;
    Ok((row, def, run))
}

pub(crate) async fn running(
    state: &AppState,
    key: &str,
) -> Result<(FleetRow, FleetDefinition, String), DefinitionError> {
    let (row, def, run) = historical_run(state, key).await?;
    if row.state != "running" {
        return Err(DefinitionError::NotRunning {
            slug: row.slug,
            state: row.state,
        });
    }
    Ok((row, def, run))
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ResultsQuery {
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    paths: Option<String>,
    #[serde(default)]
    since: Option<DateTime<Utc>>,
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    include_output: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct ResultView {
    seat: Option<String>,
    command: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    running: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    completion_unknown: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    waiting: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    waiting_seconds: Option<i64>,
    still_valid: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    use_id: Option<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    changed_since: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    finished_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_tail: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ResultsView {
    run_id: String,
    paths: Vec<String>,
    validity_provenance: &'static str,
    results: Vec<ResultView>,
}

pub(crate) async fn get_fleet_results(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(query): Query<ResultsQuery>,
) -> Result<Json<ResultsView>, DefinitionError> {
    let (_, def, run) = historical_run(&state, &key).await?;
    let now = Utc::now();
    let window = now - Duration::hours(DEFAULT_WINDOW_HOURS);
    let since = query.since.unwrap_or(window);
    let limit = query.limit.unwrap_or(20).clamp(1, 200);
    let persist = state.persist().ok_or(DefinitionError::Unavailable)?;
    let rows = persist
        .fleet()
        .tree_results(&run, query.command.as_deref(), since, limit)
        .await?;
    let earliest = rows.iter().filter_map(|r| r.started_at).min();
    let changed = match earliest {
        Some(at) => {
            persist
                .fleet()
                .tree_changed_paths_since(&run, at - Duration::microseconds(1))
                .await?
        }
        None => Vec::new(),
    };
    let paths: Vec<String> = query
        .paths
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(normal)
        .collect();
    let queued = waiting_for_slot(persist, &def, (&run, window), &rows).await?;
    let results = rows
        .into_iter()
        .map(|row| {
            let waited = queued.contains(&row.use_id).then(|| {
                row.started_at
                    .map_or(0, |at| (now - at).num_seconds().max(0))
            });
            result_view(row, &changed, &paths, query.include_output).waited(waited)
        })
        .collect();
    Ok(Json(ResultsView {
        run_id: run,
        paths,
        validity_provenance: RESULT_VALIDITY_PROVENANCE,
        results,
    }))
}

async fn waiting_for_slot(
    persist: &PersistHandle,
    def: &FleetDefinition,
    (run, window): (&str, DateTime<Utc>),
    rows: &[TreeResultRow],
) -> Result<BTreeSet<i64>, PersistError> {
    if rows.iter().all(|r| r.finished_at.is_some()) {
        return Ok(BTreeSet::new());
    }
    let open = persist
        .fleet()
        .tree_results(run, None, window, OPEN_READ)
        .await?;
    Ok(waiting_uses(&open, &def.wrappers, &def.execution))
}

impl ResultView {
    fn waited(mut self, seconds: Option<i64>) -> Self {
        if seconds.is_some() && !self.completion_unknown {
            self.running = false;
            self.waiting = true;
            self.waiting_seconds = seconds;
        }
        self
    }
}

pub(crate) fn result_view(
    row: TreeResultRow,
    changed: &[crate::store::persistence::fleet::ChangedPathRow],
    paths: &[String],
    include_output: bool,
) -> ResultView {
    let mut changed_since: Vec<String> = row
        .started_at
        .map(|started| {
            changed
                .iter()
                .filter(|c| c.seen_at >= started)
                .filter(|c| paths.is_empty() || covers(paths, &c.path))
                .map(|c| c.path.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect()
        })
        .unwrap_or_default();
    let still_valid =
        crate::launch::fleet::report_proof::captured_result_validity(&row, Some(changed));
    changed_since.truncate(CHANGED_SHOWN);
    let completion_unknown = crate::launch::fleet::background_result::completion_unknown(&row);
    let running = row.finished_at.is_none() && !completion_unknown;
    ResultView {
        seat: row.seat,
        command: row.command,
        running,
        completion_unknown,
        waiting: false,
        waiting_seconds: None,
        still_valid,
        use_id: include_output.then_some(row.use_id),
        changed_since,
        session: include_output
            .then(|| row.session_uuid.map(|u| u.to_string()))
            .flatten(),
        started_at: include_output.then_some(row.started_at).flatten(),
        finished_at: include_output.then_some(row.finished_at).flatten(),
        is_error: include_output.then_some(row.is_error).flatten(),
        output_tail: include_output.then(|| tail(row.content.as_ref())),
    }
}

pub(crate) fn tail(content: Option<&Value>) -> String {
    let text = match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(TAIL_LINES);
    lines[start..].join("\n")
}

pub(crate) async fn commit_fleet(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(request): Json<CommitRequest>,
) -> Result<Json<WithTail<CommitReport>>, DefinitionError> {
    let (_, def, run) = running(&state, &key).await?;
    let verifier = Verifier {
        persist: state.persist(),
        run: &run,
    };
    let seated = match state.persist() {
        Some(persist) => Seated::of_run(persist, &run, &def).await,
        None => Seated::default(),
    };
    let report = commit(&def, &seated, &request, verifier).await;
    if let Some(persist) = state.persist() {
        commit_note::write(persist, request.session, &request.message, &report).await;
        if let Some(words) = &report.push_error {
            crate::launch::fleet::push::tell_coordinator(persist, &def, &run, words).await;
        }
    }
    let since_you_last_looked = since_you_last_looked(&state, &def, &run, request.session).await;
    Ok(Json(WithTail {
        report,
        since_you_last_looked,
    }))
}

pub(crate) async fn integrate_fleet(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<IntegrateReport>, DefinitionError> {
    let (_, def, _) = running(&state, &key).await?;
    Ok(Json(integrate(&def).await))
}

#[cfg(test)]
#[path = "tree_result_tests.rs"]
mod tests;
