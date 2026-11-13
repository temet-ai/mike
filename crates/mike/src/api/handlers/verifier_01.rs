use super::*;
use crate::store::persistence::steerer_repo::SteererPersist as _;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct VerifierRateDto {
    pub(crate) rows: Vec<VerifierRateEntryDto>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct VerifierRateEntryDto {
    pub(crate) project_path: String,
    pub(crate) fires_count: i64,
    pub(crate) corrected_count: i64,
    pub(crate) delivered_count: i64,
    pub(crate) excluded_judge_failed_count: i64,
    pub(crate) corrected_rate: Option<f64>,
    pub(crate) delivered_rate: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct VerifierRateParams {
    #[serde(default)]
    pub(crate) project: Option<String>,
}

pub(crate) async fn get_verifier_rate(
    State(state): State<AppState>,
    Query(params): Query<VerifierRateParams>,
) -> Result<Json<VerifierRateDto>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let rows = handle
        .steerer()
        .fetch_verifier_rate(params.project.as_deref())
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "get_verifier_rate: query failed");
            persist_error_response(e)
        })?;
    let entries = rows
        .into_iter()
        .map(|r| VerifierRateEntryDto {
            project_path: r.project_path,
            fires_count: r.fires_count,
            corrected_count: r.corrected_count,
            delivered_count: r.delivered_count,
            excluded_judge_failed_count: r.excluded_judge_failed_count,
            corrected_rate: r.corrected_rate,
            delivered_rate: r.delivered_rate,
        })
        .collect();
    Ok(Json(VerifierRateDto { rows: entries }))
}

#[derive(Debug, Serialize)]
pub(crate) struct SteererDto {
    pub(crate) id: i64,
    pub(crate) uuid: uuid::Uuid,
    pub(crate) name: String,
    pub(crate) display_name: String,
    pub(crate) description: String,
    pub(crate) agent_id: i64,
    pub(crate) authority: String,
    pub(crate) mode: String,
    pub(crate) model: String,
    pub(crate) effective_model: String,
    pub(crate) model_inherited: bool,
    pub(crate) priority: i32,
    pub(crate) enabled: bool,
    pub(crate) config: serde_json::Value,
    pub(crate) prompt: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) kind: String,
    pub(crate) trigger_point: String,
    pub(crate) invocation: String,
    pub(crate) pattern: Option<String>,
    pub(crate) auto_enrol: bool,
    pub(crate) enrolment_required: bool,
    pub(crate) project_id: Option<i64>,
    pub(crate) project_path: Option<String>,
    pub(crate) active_enrolment_count: i64,
}
