use super::*;
use crate::store::persistence::proxy_repo::ProxyPersist as _;

#[derive(Debug, Serialize)]
pub struct FilterRuleEnvelope {
    pub rule: FilterRuleDto,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct FilterRuleCreate {
    #[serde(default)]
    pub id: Option<String>,
    pub kind: String,
    pub value: String,
    pub action: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub trigger_token: Option<String>,
    #[serde(default)]
    pub inject_text: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub project_path: Option<String>,
    #[serde(default)]
    pub style_override: bool,
    #[serde(default)]
    pub title: Option<String>,
}

pub fn default_enabled() -> bool {
    true
}

pub fn default_trigger_point() -> String {
    "response".to_string()
}

pub fn default_invocation() -> String {
    "every_turn".to_string()
}

#[derive(Debug, Deserialize)]
pub struct FilterRulePatch {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string_field")]
    pub text: OptionalField<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string_field")]
    pub trigger_token: OptionalField<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string_field")]
    pub inject_text: OptionalField<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional_string_field")]
    pub project_path: OptionalField<String>,
    #[serde(default)]
    pub sort_order: Option<i32>,
    #[serde(default)]
    pub style_override: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional_string_field")]
    pub title: OptionalField<String>,
}

#[derive(Debug, Default)]
pub enum OptionalField<T> {
    #[default]
    Absent,
    Null,
    Some(T),
}

pub fn deserialize_optional_string_field<'de, D>(
    de: D,
) -> Result<OptionalField<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(de)?;
    Ok(match opt {
        Some(s) => OptionalField::Some(s),
        None => OptionalField::Null,
    })
}

#[derive(Debug, Serialize)]
pub struct FilterRuleErrorBody {
    pub error: String,
}

pub fn filter_error_response(
    status: StatusCode,
    error: &str,
) -> (StatusCode, Json<FilterRuleErrorBody>) {
    (
        status,
        Json(FilterRuleErrorBody {
            error: error.to_string(),
        }),
    )
}

pub async fn list_filter_rules(State(state): State<AppState>) -> Json<Vec<FilterRuleDto>> {
    let cfg = state.filter_config.load_full();

    let firing_stats = match state.persist() {
        Some(handle) => handle
            .proxy()
            .fetch_filter_rule_firing_stats_all()
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(
                    error = %e,
                    "list_filter_rules: firing-stats aggregate failed; defaulting to zero counts"
                );
                std::collections::HashMap::new()
            }),
        None => std::collections::HashMap::new(),
    };

    let dtos: Vec<FilterRuleDto> = cfg
        .rules
        .iter()
        .map(|r| {
            let mut dto = FilterRuleDto::from(r);
            let stats = firing_stats.get(&r.id).cloned().unwrap_or_default();
            dto.last_fired_at = stats.last_fired_at.map(|t| t.to_rfc3339());
            dto.fires_24h = stats.fires_24h;
            dto.fires_7d = stats.fires_7d;
            dto
        })
        .collect();
    Json(dtos)
}

pub async fn create_filter_rule(
    State(state): State<AppState>,
    Json(body): Json<FilterRuleCreate>,
) -> Result<(StatusCode, Json<FilterRuleEnvelope>), (StatusCode, Json<FilterRuleErrorBody>)> {
    let Some(svc) = state.filter_service() else {
        return Err(filter_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; filter rule mutation is not available",
        ));
    };
    let input = crate::filter::service::FilterRuleCreateInput {
        kind: body.kind,
        value: body.value,
        action: body.action,
        text: body.text,
        trigger_token: body.trigger_token,
        inject_text: body.inject_text,
        enabled: body.enabled,
        project_path: body.project_path,
        style_override: body.style_override,
        title: body.title,
    };
    match svc.create(input).await {
        Ok(envelope) => Ok((
            StatusCode::CREATED,
            Json(FilterRuleEnvelope {
                rule: FilterRuleDto::from(&envelope.rule),
                warnings: envelope.warnings,
            }),
        )),
        Err(e) => Err(map_filter_service_error(e)),
    }
}

pub async fn patch_filter_rule(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<FilterRulePatch>,
) -> Result<Json<FilterRuleEnvelope>, (StatusCode, Json<FilterRuleErrorBody>)> {
    let Some(svc) = state.filter_service() else {
        return Err(filter_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; filter rule mutation is not available",
        ));
    };
    let numeric_id = id.parse::<i64>().map_err(|_| {
        filter_error_response(
            StatusCode::BAD_REQUEST,
            "filter rule id must be a valid integer",
        )
    })?;
    if !state
        .filter_config
        .load_full()
        .rules
        .iter()
        .any(|r| r.id == numeric_id)
    {
        return Err(filter_error_response(
            StatusCode::NOT_FOUND,
            "no filter rule with that id",
        ));
    }
    let id = numeric_id;
    let input = crate::filter::service::FilterRulePatchInput {
        kind: body.kind,
        value: body.value,
        action: body.action,
        text: match body.text {
            OptionalField::Absent => crate::filter::service::OptionalField::Absent,
            OptionalField::Null => crate::filter::service::OptionalField::Null,
            OptionalField::Some(t) => crate::filter::service::OptionalField::Some(t),
        },
        trigger_token: match body.trigger_token {
            OptionalField::Absent => crate::filter::service::OptionalField::Absent,
            OptionalField::Null => crate::filter::service::OptionalField::Null,
            OptionalField::Some(t) => crate::filter::service::OptionalField::Some(t),
        },
        inject_text: match body.inject_text {
            OptionalField::Absent => crate::filter::service::OptionalField::Absent,
            OptionalField::Null => crate::filter::service::OptionalField::Null,
            OptionalField::Some(t) => crate::filter::service::OptionalField::Some(t),
        },
        enabled: body.enabled,
        project_path: match body.project_path {
            OptionalField::Absent => crate::filter::service::OptionalField::Absent,
            OptionalField::Null => crate::filter::service::OptionalField::Null,
            OptionalField::Some(p) => crate::filter::service::OptionalField::Some(p),
        },
        sort_order: body.sort_order,
        style_override: body.style_override,
        title: match body.title {
            OptionalField::Absent => crate::filter::service::OptionalField::Absent,
            OptionalField::Null => crate::filter::service::OptionalField::Null,
            OptionalField::Some(t) => crate::filter::service::OptionalField::Some(t),
        },
    };
    match svc.update(id, input).await {
        Ok(envelope) => {
            let mut dto = FilterRuleDto::from(&envelope.rule);
            let stats = match state.persist() {
                Some(handle) => handle
                    .proxy().fetch_filter_rule_firing_stats(id)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::warn!(
                            error = %e,
                            rule_id = id,
                            "patch_filter_rule: firing-stats query failed; defaulting to zero counts"
                        );
                        Default::default()
                    }),
                None => Default::default(),
            };
            dto.last_fired_at = stats.last_fired_at.map(|t| t.to_rfc3339());
            dto.fires_24h = stats.fires_24h;
            dto.fires_7d = stats.fires_7d;
            Ok(Json(FilterRuleEnvelope {
                rule: dto,
                warnings: envelope.warnings,
            }))
        }
        Err(e) => Err(map_filter_service_error(e)),
    }
}

pub async fn delete_filter_rule(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, (StatusCode, Json<FilterRuleErrorBody>)> {
    let Some(svc) = state.filter_service() else {
        return Err(filter_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled; filter rule mutation is not available",
        ));
    };
    let numeric_id = id.parse::<i64>().map_err(|_| {
        filter_error_response(
            StatusCode::BAD_REQUEST,
            "filter rule id must be a valid integer",
        )
    })?;
    if !state
        .filter_config
        .load_full()
        .rules
        .iter()
        .any(|r| r.id == numeric_id)
    {
        return Err(filter_error_response(
            StatusCode::NOT_FOUND,
            "no filter rule with that id",
        ));
    }
    match svc.delete(numeric_id).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(e) => Err(map_filter_service_error(e)),
    }
}

pub fn map_filter_service_error(
    e: crate::filter::service::FilterServiceError,
) -> (StatusCode, Json<FilterRuleErrorBody>) {
    use crate::filter::service::FilterServiceError;
    match e {
        FilterServiceError::Validation(msg) | FilterServiceError::Parse(msg) => {
            filter_error_response(StatusCode::BAD_REQUEST, &msg)
        }
        FilterServiceError::NotFound => {
            filter_error_response(StatusCode::NOT_FOUND, "no filter rule with that id")
        }
        FilterServiceError::Persistence(persist_err) => {
            tracing::error!(error = %persist_err, "filter rule persistence failure");
            filter_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database error while processing filter rule",
            )
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct FilterDryRunRequest {
    pub request_id: u64,
    #[serde(default)]
    pub rules: Option<Vec<FilterRuleCreate>>,
}

#[derive(Debug, Serialize)]
pub struct FilterDryRunResponse {
    pub request_id: u64,
    pub changed: bool,
    pub project_path: String,
    pub firing_rule_ids: Vec<i64>,
    pub original_len: usize,
    pub filtered_len: usize,
    pub filtered_system: Option<serde_json::Value>,
    pub filtered_messages: Option<serde_json::Value>,
}
