use super::*;
use crate::store::persistence::proxy_repo::ProxyPersist as _;

#[derive(Debug, Serialize)]
pub struct PromptSubPatternDto {
    pub name: &'static str,
    pub description: &'static str,
    pub recommended_action: &'static str,
}

#[derive(Debug, Serialize)]
pub struct PromptAnomalyObservationDto {
    pub request_id: i64,
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct PromptAnomalyGroup {
    pub classification: &'static str,
    pub signature_hash: String,
    pub prompt_sha256: Option<String>,
    pub block_text: Option<String>,
    pub unknown_blocks: Option<serde_json::Value>,
    pub count: i64,
    pub first_seen: String,
    pub last_seen: String,
    pub observations: Vec<PromptAnomalyObservationDto>,
    pub sub_pattern: PromptSubPatternDto,
    pub firing_rule_ids: Vec<String>,
    pub persona: Option<String>,
}

pub const ANOMALIES_DEFAULT_LIMIT: i64 = 200;
pub const ANOMALIES_MAX_LIMIT: i64 = 500;

#[derive(Debug, Default, Deserialize)]
pub struct PromptAnomaliesParams {
    #[serde(default)]
    pub date_from: Option<String>,
    #[serde(default)]
    pub date_to: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

pub fn parse_rfc3339_query_date(
    raw: Option<&str>,
    field: &'static str,
) -> Option<chrono::DateTime<chrono::Utc>> {
    raw.and_then(|s| match chrono::DateTime::parse_from_rfc3339(s) {
        Ok(dt) => Some(dt.with_timezone(&chrono::Utc)),
        Err(e) => {
            tracing::warn!(%field, value = %s, error = %e, "invalid RFC 3339 date query param; ignoring");
            None
        }
    })
}

pub async fn list_prompt_anomalies(
    State(state): State<AppState>,
    Query(params): Query<PromptAnomaliesParams>,
) -> Result<Json<Vec<PromptAnomalyGroup>>, (StatusCode, Json<PromptCompositionErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Ok(Json(Vec::new()));
    };
    let date_from = parse_rfc3339_query_date(params.date_from.as_deref(), "date_from");
    let date_to = parse_rfc3339_query_date(params.date_to.as_deref(), "date_to");
    let limit = params
        .limit
        .unwrap_or(ANOMALIES_DEFAULT_LIMIT)
        .clamp(1, ANOMALIES_MAX_LIMIT);
    let offset = params.offset.unwrap_or(0).max(0);
    let groups = handle
        .proxy()
        .list_anomalies_windowed(date_from, date_to, limit, offset)
        .await
        .map_err(map_composition_persist_error)?;
    let anomalies: Vec<PromptAnomalyGroup> = groups
        .into_iter()
        .map(|group| PromptAnomalyGroup {
            classification: group.classification.as_str(),
            signature_hash: group.signature_hash,
            prompt_sha256: group.prompt_sha256,
            block_text: group.block_text,
            unknown_blocks: group.unknown_blocks,
            count: group.count,
            first_seen: group.first_seen.to_rfc3339(),
            last_seen: group.last_seen.to_rfc3339(),
            observations: group
                .observations
                .into_iter()
                .map(|o| PromptAnomalyObservationDto {
                    request_id: o.request_id,
                    session_id: o.session_id,
                })
                .collect(),
            sub_pattern: PromptSubPatternDto {
                name: group.sub_pattern.name(),
                description: group.sub_pattern.description(),
                recommended_action: group.sub_pattern.recommended_action(),
            },
            firing_rule_ids: group.firing_rule_ids,
            persona: group.persona,
        })
        .collect();
    Ok(Json(anomalies))
}
