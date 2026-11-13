use super::*;

pub(crate) use crate::web_base::http::if_none_match_contains;

pub(crate) fn session_etag(
    last_seen: &str,
    request_count: usize,
    busy: bool,
    session_judged_at: Option<&str>,
) -> String {
    format!(
        "W/\"{}-{}-{}-{}\"",
        last_seen,
        request_count,
        busy,
        session_judged_at.unwrap_or("unjudged")
    )
}

pub(crate) fn status_is_terminal(last_status: &str) -> bool {
    last_status.starts_with("Complete") || last_status.starts_with("Error")
}

pub(crate) fn compute_session_busy(in_registry: bool, last_status: &str) -> bool {
    in_registry || !status_is_terminal(last_status)
}

#[derive(Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
pub(crate) struct SessionSummary {
    pub(crate) id: String,
    pub(crate) first_seen: String,
    pub(crate) last_seen: String,
    pub(crate) request_count: usize,
    pub(crate) models: Vec<String>,
    pub(crate) last_status: String,
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) project_id: Option<i64>,
    #[serde(default)]
    pub(crate) cwd: Option<String>,
    #[serde(default)]
    pub(crate) project_resolution: String,
    pub(crate) git_repo: Option<String>,
    pub(crate) pinned: bool,
    pub(crate) ttl_override: Option<String>,
    pub(crate) busy: bool,
    #[serde(default)]
    pub(crate) title: Option<String>,
    #[serde(default)]
    pub(crate) avg_latency_ms: Option<f64>,
    #[serde(default)]
    pub(crate) originating_operator_prompt: Option<String>,
    #[serde(default)]
    pub(crate) session_outcome: Option<String>,
    #[serde(default)]
    pub(crate) session_quality_score: Option<f64>,
    #[serde(default)]
    pub(crate) session_judged_at: Option<String>,
    #[serde(default)]
    pub(crate) narrative_overview: Option<String>,
    #[serde(default)]
    pub(crate) narrative_overview_generated_at: Option<String>,
    #[serde(default)]
    pub(crate) pending_inbox_count: usize,
    #[serde(default)]
    pub(crate) projects_touched: Vec<String>,
}

impl SessionSummary {
    pub(crate) const FALLBACK_OMITTED_FIELDS: &'static [&'static str] = &[
        "ttl_override",
        "project_id",
        "cwd",
        "project_resolution",
        "narrative_overview",
        "narrative_overview_generated_at",
        "pending_inbox_count",
        "projects_touched",
    ];
}

impl From<&SessionView> for SessionSummary {
    fn from(view: &SessionView) -> Self {
        Self {
            id: view.id.clone(),
            first_seen: view.first_seen.to_rfc3339(),
            last_seen: view.last_seen.to_rfc3339(),
            request_count: view.request_count,
            models: view.models.clone(),
            last_status: view.last_status.clone(),
            project: None,
            project_id: None,
            cwd: None,
            project_resolution: "unresolved".to_string(),
            git_repo: None,
            pinned: view.pinned,
            ttl_override: view.ttl_override.clone(),
            title: view.title.clone(),
            avg_latency_ms: None,
            originating_operator_prompt: None,
            session_outcome: None,
            session_quality_score: None,
            session_judged_at: None,
            narrative_overview: None,
            narrative_overview_generated_at: None,
            pending_inbox_count: 0,
            projects_touched: Vec::new(),
            busy: !status_is_terminal(&view.last_status),
        }
    }
}

impl From<&SessionSummaryRow> for SessionSummary {
    fn from(row: &SessionSummaryRow) -> Self {
        debug_assert!(
            !SessionSummary::FALLBACK_OMITTED_FIELDS.is_empty(),
            "FALLBACK_OMITTED_FIELDS is empty — if every field now round-trips \
             through the DB-fallback path the parity matrix test should be \
             updated and the constant removed, not left as an empty slice"
        );
        let project = if row.project_path == "(no project)" {
            None
        } else {
            Some(row.project_path.clone())
        };
        Self {
            id: row.session_id.clone(),
            first_seen: row.first_seen.to_rfc3339(),
            last_seen: row.last_seen.to_rfc3339(),
            request_count: usize::try_from(row.request_count.max(0)).unwrap_or(0),
            models: row.models(),
            last_status: row.last_status.clone().unwrap_or_default(),
            project,
            project_id: row.project_id,
            cwd: row.cwd.clone(),
            project_resolution: row.project_resolution.clone(),
            git_repo: None,
            pinned: row.effective_pinned(),
            ttl_override: None,
            title: row.title.clone(),
            avg_latency_ms: row.avg_latency_ms,
            originating_operator_prompt: row.originating_operator_prompt.clone(),
            session_outcome: row.session_outcome.clone(),
            session_quality_score: row.session_quality_score,
            session_judged_at: row.session_judged_at.map(|dt| dt.to_rfc3339()),
            narrative_overview: row.overview.clone(),
            narrative_overview_generated_at: row.overview_generated_at.map(|dt| dt.to_rfc3339()),
            pending_inbox_count: usize::try_from(row.pending_inbox_count.max(0)).unwrap_or(0),
            projects_touched: row.projects_touched.clone(),
            busy: !status_is_terminal(&row.last_status.clone().unwrap_or_default()),
        }
    }
}
