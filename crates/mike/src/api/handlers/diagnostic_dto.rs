use super::*;

#[derive(Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export_to = "../../frontend/src/lib/api/generated/")
)]
pub(crate) struct DiagnosticDto {
    pub(crate) category: DiagnosticCategory,
    pub(crate) analyzer: String,
    pub(crate) advice: String,
    pub(crate) matched_text: String,
    pub(crate) match_range: (usize, usize),
    pub(crate) other_hits: usize,
    pub(crate) created_at: String,
}

impl From<&Diagnostic> for DiagnosticDto {
    fn from(d: &Diagnostic) -> Self {
        Self {
            category: d.category.clone(),
            analyzer: d.analyzer.clone(),
            advice: d.advice.clone(),
            matched_text: d.matched_text.clone(),
            match_range: d.match_range,
            other_hits: d.other_hits,
            created_at: d.created_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct DiagnosticStats {
    pub(crate) totals_by_category: BTreeMap<DiagnosticCategory, usize>,
    pub(crate) totals_by_analyzer: BTreeMap<String, usize>,
    pub(crate) last_24h: usize,
}

impl From<&DiagnosticStatsView> for DiagnosticStats {
    fn from(view: &DiagnosticStatsView) -> Self {
        Self {
            totals_by_category: view.totals_by_category.clone(),
            totals_by_analyzer: view.totals_by_analyzer.clone(),
            last_24h: view.last_24h,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct FindingRequestRef {
    pub(crate) model: Option<String>,
    pub(crate) status: String,
    pub(crate) path: String,
    pub(crate) project: Option<String>,
    pub(crate) human_prompt: String,
    pub(crate) created_at: String,
}

#[derive(Serialize)]
pub(crate) struct FindingDto {
    pub(crate) request_id: u64,
    pub(crate) session_id: String,
    pub(crate) category: DiagnosticCategory,
    pub(crate) analyzer: String,
    pub(crate) advice: String,
    pub(crate) matched_text: String,
    pub(crate) match_range: (usize, usize),
    pub(crate) other_hits: usize,
    pub(crate) created_at: String,
    pub(crate) request: FindingRequestRef,
}

impl From<&FindingView> for FindingDto {
    fn from(view: &FindingView) -> Self {
        Self {
            request_id: view.request_id.as_u64(),
            session_id: view.session_id.clone(),
            category: view.diagnostic.category.clone(),
            analyzer: view.diagnostic.analyzer.clone(),
            advice: view.diagnostic.advice.clone(),
            matched_text: view.diagnostic.matched_text.clone(),
            match_range: view.diagnostic.match_range,
            other_hits: view.diagnostic.other_hits,
            created_at: view.diagnostic.created_at.to_rfc3339(),
            request: FindingRequestRef {
                model: view.request.model.clone(),
                status: view.request.status.clone(),
                path: view.request.path.clone(),
                project: view.request.project.clone(),
                human_prompt: view.request.human_prompt.clone(),
                created_at: view.request.created_at.to_rfc3339(),
            },
        }
    }
}

pub(crate) const FINDINGS_LIST_DEFAULT_LIMIT: usize = 50;
pub(crate) const FINDINGS_LIST_MAX_LIMIT: usize = 200;

#[derive(Serialize)]
pub(crate) struct FindingsPage {
    pub(crate) findings: Vec<FindingDto>,
    pub(crate) total: u64,
    pub(crate) offset: usize,
    pub(crate) limit: usize,
}

#[derive(Deserialize)]
pub(crate) struct FindingsParams {
    pub(crate) category: Option<DiagnosticCategory>,
    pub(crate) analyzer: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) project: Option<String>,
    pub(crate) offset: Option<usize>,
    pub(crate) limit: Option<usize>,
    #[serde(default)]
    pub(crate) total: bool,
    pub(crate) order: Option<SortOrder>,
}

#[derive(Deserialize)]
pub(crate) struct DiagnosticStatsParams {
    pub(crate) project: Option<String>,
}
