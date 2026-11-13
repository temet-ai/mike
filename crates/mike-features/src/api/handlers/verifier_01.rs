use super::*;
use crate::store::persistence::steerer_repo::SteererPersist as _;

#[derive(Debug, Clone, Serialize)]
pub struct VerifierRateDto {
    pub rows: Vec<VerifierRateEntryDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifierRateEntryDto {
    pub project_path: String,
    pub fires_count: i64,
    pub corrected_count: i64,
    pub delivered_count: i64,
    pub excluded_judge_failed_count: i64,
    pub corrected_rate: Option<f64>,
    pub delivered_rate: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct VerifierRateParams {
    #[serde(default)]
    pub project: Option<String>,
}

pub async fn get_verifier_rate(
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
pub struct SteererDto {
    pub id: i64,
    pub uuid: uuid::Uuid,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub agent_id: i64,
    pub authority: String,
    pub mode: String,
    pub model: String,
    pub effective_model: String,
    pub model_inherited: bool,
    pub priority: i32,
    pub enabled: bool,
    pub config: serde_json::Value,
    pub prompt: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub kind: String,
    pub trigger_point: String,
    pub invocation: String,
    pub pattern: Option<String>,
    pub auto_enrol: bool,
    pub enrolment_required: bool,
    pub project_id: Option<i64>,
    pub project_path: Option<String>,
    pub active_enrolment_count: i64,
}

impl From<&crate::store::persistence::steering::SteererRow> for SteererDto {
    fn from(r: &crate::store::persistence::steering::SteererRow) -> Self {
        Self {
            id: r.id,
            uuid: r.uuid,
            name: r.name.clone(),
            display_name: r.display_name.clone(),
            description: r.description.clone(),
            agent_id: r.agent_id,
            authority: r.authority.clone(),
            mode: r.mode.clone(),
            model: r.model.clone(),
            effective_model: r.model.clone(),
            model_inherited: r.model.is_empty(),
            priority: r.priority,
            enabled: r.enabled,
            config: r.config.clone(),
            prompt: r.prompt.clone(),
            created_at: r.created_at.to_rfc3339(),
            updated_at: r.updated_at.to_rfc3339(),
            kind: r.kind.clone(),
            trigger_point: r.trigger_point.clone(),
            invocation: r.invocation.clone(),
            pattern: r.pattern.clone(),
            auto_enrol: r.auto_enrol,
            enrolment_required: !r.auto_enrol,
            project_id: r.project_id,
            project_path: r.project_path.clone(),
            active_enrolment_count: 0,
        }
    }
}
