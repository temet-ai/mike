mod group;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration as StdDuration, Instant};

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::state::AppState;
use crate::store::persistence::improvement::repeats::PushbackPrompt;

const DEFAULT_WEEKS: i64 = 12;
const DEFAULT_MIN_COUNT: usize = 3;
const DEFAULT_THRESHOLD: f32 = 0.84;
const MIN_SPAN_DAYS: i64 = 7;
const LIVE_DAYS: i64 = 14;
const CACHE_TTL: StdDuration = StdDuration::from_secs(15 * 60);

#[derive(Debug, Default, Deserialize)]
pub(crate) struct RepeatsQuery {
    pub weeks: Option<i64>,
    pub min: Option<usize>,
    pub threshold: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RepeatItem {
    pub turn_id: i64,
    pub session_id: Uuid,
    pub at: DateTime<Utc>,
    pub project: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RepeatGroup {
    pub id: i64,
    pub n: usize,
    pub first_at: DateTime<Utc>,
    pub last_at: DateTime<Utc>,
    pub projects: Vec<String>,
    pub weekly: Vec<u32>,
    pub live: bool,
    pub items: Vec<RepeatItem>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RepeatsResponse {
    pub weeks: i64,
    pub threshold: f32,
    pub min_count: usize,
    pub turns: usize,
    pub duplicates: usize,
    pub computed_at: DateTime<Utc>,
    pub week_starts: Vec<NaiveDate>,
    pub groups: Vec<RepeatGroup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CacheKey {
    weeks: i64,
    min_count: usize,
    threshold_bits: u32,
}

type Cache = Mutex<HashMap<CacheKey, (Instant, Arc<RepeatsResponse>)>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached(key: CacheKey) -> Option<Arc<RepeatsResponse>> {
    let guard = match cache().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard
        .get(&key)
        .filter(|(at, _)| at.elapsed() < CACHE_TTL)
        .map(|(_, r)| Arc::clone(r))
}

fn remember(key: CacheKey, response: Arc<RepeatsResponse>) {
    let mut guard = match cache().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.insert(key, (Instant::now(), response));
}

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn project_name(path: &str) -> String {
    path.rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or("(no project)")
        .to_owned()
}

fn item(p: &PushbackPrompt) -> RepeatItem {
    RepeatItem {
        turn_id: p.turn_id,
        session_id: p.session_id,
        at: p.at,
        project: project_name(&p.project_path),
        text: p.text.clone(),
    }
}

const MACHINE_OPENERS: [&str; 8] = [
    "Coordinator wake",
    "coordinator wake",
    "Acceptance seat",
    "acceptance pass:",
    "observer deep cycle",
    "Stop hook feedback:",
    "A session-scoped Stop hook",
    "Output token limit hit",
];

fn is_machine_written(text: &str) -> bool {
    let t = text.trim_start();
    MACHINE_OPENERS.iter().any(|opener| t.starts_with(opener))
}

fn first_of_each_text_per_session(prompts: &[PushbackPrompt]) -> Vec<&PushbackPrompt> {
    let mut seen = HashSet::new();
    prompts
        .iter()
        .filter(|p| !is_machine_written(&p.text))
        .filter(|p| seen.insert((p.session_id, p.text.trim())))
        .collect()
}

fn build_group(
    prompts: &[&PushbackPrompt],
    members: &[usize],
    starts: &[DateTime<Utc>],
    now: DateTime<Utc>,
) -> RepeatGroup {
    let mut ordered: Vec<&PushbackPrompt> = members.iter().map(|&i| prompts[i]).collect();
    ordered.sort_by_key(|p| (p.at, p.turn_id));
    let ats: Vec<DateTime<Utc>> = ordered.iter().map(|p| p.at).collect();
    let mut projects: Vec<String> = ordered
        .iter()
        .map(|p| project_name(&p.project_path))
        .collect();
    projects.sort();
    projects.dedup();
    let first_at = ats[0];
    let last_at = ats[ats.len() - 1];
    RepeatGroup {
        id: ordered.iter().map(|p| p.turn_id).min().unwrap_or(0),
        n: ordered.len(),
        first_at,
        last_at,
        projects,
        weekly: group::weekly_counts(&ats, starts),
        live: now - last_at <= Duration::days(LIVE_DAYS),
        items: ordered.iter().map(|p| item(p)).collect(),
    }
}

fn keeps(group: &RepeatGroup, min_count: usize) -> bool {
    let span = (group.last_at - group.first_at).num_days();
    group.n >= min_count && (span >= MIN_SPAN_DAYS || group.projects.len() >= 2)
}

pub(crate) fn compute(
    prompts: &[PushbackPrompt],
    now: DateTime<Utc>,
    weeks: i64,
    min_count: usize,
    threshold: f32,
) -> RepeatsResponse {
    let starts = group::week_starts(now, weeks);
    let kept = first_of_each_text_per_session(prompts);
    let vectors: Vec<&[f32]> = kept.iter().map(|p| p.embedding.as_slice()).collect();
    let sim = group::cosine_matrix(&vectors);
    let mut groups: Vec<RepeatGroup> = group::star_groups(&sim, threshold)
        .iter()
        .map(|members| build_group(&kept, members, &starts, now))
        .filter(|g| keeps(g, min_count))
        .collect();
    groups.sort_by(|a, b| b.n.cmp(&a.n).then(a.id.cmp(&b.id)));
    RepeatsResponse {
        weeks,
        threshold,
        min_count,
        turns: kept.len(),
        duplicates: prompts.len() - kept.len(),
        computed_at: now,
        week_starts: starts.iter().map(|s| s.date_naive()).collect(),
        groups,
    }
}

pub(crate) async fn get_repeats(
    State(state): State<AppState>,
    Query(q): Query<RepeatsQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled".to_owned(),
        );
    };
    let weeks = q.weeks.unwrap_or(DEFAULT_WEEKS).clamp(1, 52);
    let min_count = q.min.unwrap_or(DEFAULT_MIN_COUNT).clamp(2, 50);
    let threshold = q.threshold.unwrap_or(DEFAULT_THRESHOLD).clamp(0.5, 0.99);
    let key = CacheKey {
        weeks,
        min_count,
        threshold_bits: threshold.to_bits(),
    };
    if let Some(hit) = cached(key) {
        return Json(hit.as_ref()).into_response();
    }
    let now = Utc::now();
    let since = now - Duration::weeks(weeks);
    let prompts = match persist.list_pushback_prompts_with_vectors(since).await {
        Ok(rows) => rows,
        Err(e) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("persistence error: {e}"),
            );
        }
    };
    let response = Arc::new(compute(&prompts, now, weeks, min_count, threshold));
    remember(key, Arc::clone(&response));
    Json(response.as_ref()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn prompt(turn_id: i64, day: u32, project: &str, text: &str, v: [f32; 2]) -> PushbackPrompt {
        let norm = (v[0] * v[0] + v[1] * v[1]).sqrt();
        PushbackPrompt {
            turn_id,
            session_id: Uuid::nil(),
            at: Utc.with_ymd_and_hms(2026, 10, day, 9, 0, 0).unwrap(),
            project_path: format!("/mnt/data/projects/{project}"),
            text: text.to_owned(),
            embedding: vec![v[0] / norm, v[1] / norm],
        }
    }

    #[test]
    fn compute_groups_the_same_ask_across_a_week_and_drops_a_one_day_pair() {
        let now = Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap();
        let prompts = vec![
            prompt(1, 1, "gg", "commit and push", [1.0, 0.0]),
            prompt(2, 8, "gg", "commit and push everything", [1.0, 0.05]),
            prompt(3, 15, "mike", "push it", [1.0, 0.1]),
            prompt(4, 15, "mike", "fix the colours", [0.0, 1.0]),
            prompt(5, 15, "mike", "fix the colours now", [0.05, 1.0]),
            prompt(6, 15, "mike", "fix colours", [0.1, 1.0]),
        ];
        let out = compute(&prompts, now, 2, 3, 0.9);
        assert_eq!(out.turns, 6);
        assert_eq!(out.groups.len(), 1);
        let g = &out.groups[0];
        assert_eq!((g.id, g.n), (1, 3));
        assert_eq!(g.projects, vec!["gg", "mike"]);
        assert!(g.live);
        assert_eq!(g.weekly, vec![1, 1]);
        assert_eq!(g.items[0].project, "gg");
    }

    #[test]
    fn compute_keeps_a_same_day_group_when_it_spans_two_projects() {
        let now = Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap();
        let prompts = vec![
            prompt(1, 15, "gg", "where is the artifact", [1.0, 0.0]),
            prompt(
                2,
                15,
                "mike",
                "where is the artifact i asked for",
                [1.0, 0.02],
            ),
            prompt(3, 15, "mike", "artifact?", [1.0, 0.04]),
        ];
        let out = compute(&prompts, now, 4, 3, 0.9);
        assert_eq!(out.groups.len(), 1);
        assert_eq!(out.groups[0].projects.len(), 2);
    }

    #[test]
    fn compute_counts_the_same_text_once_per_session_and_again_in_another_session() {
        let now = Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap();
        let other = Uuid::from_u128(7);
        let mut prompts = vec![
            prompt(1, 1, "gg", "create a mr", [1.0, 0.0]),
            prompt(2, 1, "gg", "create a mr", [1.0, 0.0]),
            prompt(3, 1, "gg", "create a mr ", [1.0, 0.0]),
            prompt(4, 9, "gg", "create a mr", [1.0, 0.0]),
            prompt(5, 15, "gg", "create the mr", [1.0, 0.02]),
        ];
        prompts[3].session_id = other;
        let out = compute(&prompts, now, 3, 3, 0.9);
        assert_eq!((out.turns, out.duplicates), (3, 2));
        assert_eq!(out.groups.len(), 1);
        assert_eq!(out.groups[0].n, 3);
        assert_eq!(
            out.groups[0]
                .items
                .iter()
                .map(|i| i.turn_id)
                .collect::<Vec<_>>(),
            vec![1, 4, 5]
        );
    }

    #[test]
    fn compute_leaves_out_prompts_a_seat_or_hook_wrote() {
        let now = Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap();
        let prompts = vec![
            prompt(
                1,
                1,
                "gg",
                "Coordinator wake: fleet abc. Run the wake loop",
                [1.0, 0.0],
            ),
            prompt(
                2,
                8,
                "gg",
                "Coordinator wake for fleet def (0a1f434b): run the wake loop",
                [1.0, 0.0],
            ),
            prompt(3, 15, "mike", "  Stop hook feedback: not met", [1.0, 0.01]),
            prompt(
                4,
                15,
                "mike",
                "what is the status of the above?",
                [0.0, 1.0],
            ),
        ];
        let out = compute(&prompts, now, 3, 2, 0.9);
        assert_eq!((out.turns, out.duplicates), (1, 3));
        assert!(out.groups.is_empty());
    }

    #[test]
    fn project_name_takes_the_last_path_part() {
        assert_eq!(project_name("/mnt/data/projects/gg/"), "gg");
        assert_eq!(project_name(""), "(no project)");
    }
}
