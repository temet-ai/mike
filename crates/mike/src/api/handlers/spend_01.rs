use chrono::{DateTime, Utc};

use crate::store::persistence::PersistHandle;
use crate::store::persistence::llm_calls::{LlmSpendCallerRow, LlmSpendDayCallerRow, fold_callers};

use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct LlmSpendWindowDto {
    pub since: Option<String>,
    pub until: Option<String>,
    pub defaulted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LlmSpendTodayDto {
    pub spent_usd: f64,
    pub ceiling_usd: f64,
    pub ceiling_enabled: bool,
    pub alert_fraction: f64,
    pub alert_threshold_usd: Option<f64>,
    pub over_alert_threshold: bool,
    pub over_ceiling: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LlmSpendGroupDto {
    pub key: String,
    pub call_count: i64,
    pub cost_usd: f64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub reasoning_tokens: i64,
    pub subscription_call_count: i64,
}

impl From<crate::store::persistence::llm_calls::LlmSpendGroupRow> for LlmSpendGroupDto {
    fn from(row: crate::store::persistence::llm_calls::LlmSpendGroupRow) -> Self {
        Self {
            key: row.key,
            call_count: row.call_count,
            cost_usd: row.cost_usd,
            prompt_tokens: row.prompt_tokens,
            completion_tokens: row.completion_tokens,
            reasoning_tokens: row.reasoning_tokens,
            subscription_call_count: row.subscription_call_count,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct LlmSpendTotalsDto {
    pub call_count: i64,
    pub cost_usd: f64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub reasoning_tokens: i64,
    pub subscription_call_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LlmSpendCallerDto {
    pub key: String,
    pub role: String,
    pub steerer_id: Option<i64>,
    pub label: String,
    pub call_count: i64,
    pub cost_usd: f64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub reasoning_tokens: i64,
    pub avg_cost_per_call: f64,
    pub avg_prompt_tokens: f64,
    pub failed_count: i64,
    pub subscription_call_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LlmSpendDayCallerDto {
    pub day: String,
    pub key: String,
    pub cost_usd: f64,
    pub call_count: i64,
}

pub(crate) fn caller_key_and_label(
    role: &str,
    steerer_id: Option<i64>,
    steerer_name: Option<&str>,
    steerer_display_name: Option<&str>,
) -> (String, String) {
    if let Some(id) = steerer_id {
        let key = steerer_name
            .map(|name| format!("steerer:{name}"))
            .unwrap_or_else(|| format!("steerer:{id}"));
        let label = match (steerer_name, steerer_display_name) {
            (Some(_), Some(display)) => format!("Steerer \u{b7} {display}"),
            _ => format!("Steerer \u{b7} (removed #{id})"),
        };
        return (key, label);
    }
    let label = match role {
        "steerer" => "Steerer (unattributed)",
        "judge_worker" => "Turn judge",
        "session_judge" => "Session judge",
        "eval" => "Eval runs",
        "lesson_relations" => "Lesson relations",
        other => return (role.to_owned(), other.to_owned()),
    };
    (role.to_owned(), label.to_owned())
}

fn caller_row_into_dto(row: LlmSpendCallerRow) -> LlmSpendCallerDto {
    let (key, label) = caller_key_and_label(
        &row.role,
        row.steerer_id,
        row.steerer_name.as_deref(),
        row.steerer_display_name.as_deref(),
    );
    let avg_cost_per_call = if row.call_count > 0 {
        row.cost_usd / row.call_count as f64
    } else {
        0.0
    };
    let avg_prompt_tokens = if row.call_count > 0 {
        row.prompt_tokens as f64 / row.call_count as f64
    } else {
        0.0
    };
    LlmSpendCallerDto {
        key,
        role: row.role,
        steerer_id: row.steerer_id,
        label,
        call_count: row.call_count,
        cost_usd: row.cost_usd,
        prompt_tokens: row.prompt_tokens,
        completion_tokens: row.completion_tokens,
        reasoning_tokens: row.reasoning_tokens,
        avg_cost_per_call,
        avg_prompt_tokens,
        failed_count: row.failed_count,
        subscription_call_count: row.subscription_call_count,
    }
}

fn day_caller_row_into_dto(row: LlmSpendDayCallerRow) -> LlmSpendDayCallerDto {
    let (key, _label) =
        caller_key_and_label(&row.role, row.steerer_id, row.steerer_name.as_deref(), None);
    LlmSpendDayCallerDto {
        day: row.day,
        key,
        cost_usd: row.cost_usd,
        call_count: row.call_count,
    }
}

async fn fetch_caller_breakdowns(
    handle: &PersistHandle,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
) -> Result<(Vec<LlmSpendCallerDto>, Vec<LlmSpendDayCallerDto>), Response> {
    let by_day_caller_rows = handle
        .llm_spend_by_day_caller(since, until)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "get_llm_spend: llm_spend_by_day_caller failed");
            persist_error_response(e)
        })?;
    let by_caller_rows = fold_callers(&by_day_caller_rows);

    let mut by_caller: Vec<LlmSpendCallerDto> = by_caller_rows
        .into_iter()
        .map(caller_row_into_dto)
        .collect();
    by_caller.sort_by(|a, b| b.cost_usd.total_cmp(&a.cost_usd));
    let by_day_caller: Vec<LlmSpendDayCallerDto> = by_day_caller_rows
        .into_iter()
        .map(day_caller_row_into_dto)
        .collect();

    Ok((by_caller, by_day_caller))
}

#[derive(Debug, Clone, Serialize)]
pub struct LlmSpendResponse {
    pub window: LlmSpendWindowDto,
    pub today: LlmSpendTodayDto,
    pub totals: LlmSpendTotalsDto,
    pub by_day: Vec<LlmSpendGroupDto>,
    pub by_model: Vec<LlmSpendGroupDto>,
    pub by_role: Vec<LlmSpendGroupDto>,
    pub by_caller: Vec<LlmSpendCallerDto>,
    pub by_day_caller: Vec<LlmSpendDayCallerDto>,
}

pub(crate) fn llm_spend_bad_date_response(
    err: crate::web_base::search_window::DateParseError,
) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": err.to_string() })),
    )
        .into_response()
}

pub async fn get_llm_spend(
    State(state): State<AppState>,
    Query(params): Query<LlmSpendParams>,
) -> Result<Json<LlmSpendResponse>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };

    let window = crate::web_base::search_window::resolve_window(
        params.since.as_deref(),
        params.until.as_deref(),
        false,
    )
    .map_err(llm_spend_bad_date_response)?;

    let today_spent_usd = handle.sum_todays_llm_spend_usd().await.map_err(|e| {
        tracing::warn!(error = %e, "get_llm_spend: sum_todays_llm_spend_usd failed");
        persist_error_response(e)
    })?;
    let by_day = handle
        .llm_spend_by_day(window.effective_from, window.effective_to)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "get_llm_spend: llm_spend_by_day failed");
            persist_error_response(e)
        })?;
    let by_model = handle
        .llm_spend_by_model(window.effective_from, window.effective_to)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "get_llm_spend: llm_spend_by_model failed");
            persist_error_response(e)
        })?;
    let by_role = handle
        .llm_spend_by_role(window.effective_from, window.effective_to)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "get_llm_spend: llm_spend_by_role failed");
            persist_error_response(e)
        })?;
    let (by_caller, by_day_caller) =
        fetch_caller_breakdowns(handle, window.effective_from, window.effective_to).await?;

    let totals = by_day
        .iter()
        .fold(LlmSpendTotalsDto::default(), |mut acc, row| {
            acc.call_count += row.call_count;
            acc.cost_usd += row.cost_usd;
            acc.prompt_tokens += row.prompt_tokens;
            acc.completion_tokens += row.completion_tokens;
            acc.reasoning_tokens += row.reasoning_tokens;
            acc.subscription_call_count += row.subscription_call_count;
            acc
        });

    let settings = state.settings_overlay.load();
    let ceiling_usd = settings.llm_daily_spend_ceiling_usd;
    let alert_fraction = settings.llm_spend_alert_fraction;
    let ceiling_enabled = ceiling_usd > 0.0;
    let alert_threshold_usd = ceiling_enabled.then_some(ceiling_usd * alert_fraction);
    let over_ceiling = ceiling_enabled && today_spent_usd >= ceiling_usd;
    let over_alert_threshold =
        ceiling_enabled && alert_threshold_usd.is_some_and(|t| today_spent_usd >= t);

    Ok(Json(LlmSpendResponse {
        window: LlmSpendWindowDto {
            since: window.effective_from.map(|t| t.to_rfc3339()),
            until: window.effective_to.map(|t| t.to_rfc3339()),
            defaulted: window.defaulted,
        },
        today: LlmSpendTodayDto {
            spent_usd: today_spent_usd,
            ceiling_usd,
            ceiling_enabled,
            alert_fraction,
            alert_threshold_usd,
            over_alert_threshold,
            over_ceiling,
        },
        totals,
        by_day: by_day.into_iter().map(Into::into).collect(),
        by_model: by_model.into_iter().map(Into::into).collect(),
        by_role: by_role.into_iter().map(Into::into).collect(),
        by_caller,
        by_day_caller,
    }))
}

#[cfg(test)]
mod llm_spend_tests {
    use axum::extract::{Query, State};

    use super::{LlmSpendParams, get_llm_spend};
    use crate::config::{AppState, ProxyConfig};

    fn empty_params() -> LlmSpendParams {
        LlmSpendParams {
            since: None,
            until: None,
        }
    }

    #[tokio::test]
    async fn persist_disabled_returns_service_unavailable() -> anyhow::Result<()> {
        let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;

        let Err(err) = get_llm_spend(State(state), Query(empty_params())).await else {
            anyhow::bail!("persistence is disabled; the handler must not report spend data");
        };
        assert_eq!(err.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);
        Ok(())
    }

    use super::caller_key_and_label;

    #[test]
    fn caller_key_and_label_steerer_with_names_uses_display_name() {
        let (key, label) =
            caller_key_and_label("steerer", Some(6), Some("verifier"), Some("Verifier"));
        assert_eq!(key, "steerer:verifier");
        assert_eq!(label, "Steerer \u{b7} Verifier");
    }

    #[test]
    fn caller_key_and_label_steerer_id_without_names_falls_back_to_id() {
        let (key, label) = caller_key_and_label("steerer", Some(99), None, None);
        assert_eq!(key, "steerer:99");
        assert_eq!(label, "Steerer \u{b7} (removed #99)");
    }

    #[test]
    fn caller_key_and_label_steerer_name_without_display_name_still_keys_on_name() {
        let (key, label) = caller_key_and_label("steerer", Some(6), Some("verifier"), None);
        assert_eq!(
            key, "steerer:verifier",
            "the key must use the technical name even when the display name is unavailable \
             (e.g. the by_day_caller rollup, which never fetches display_name)"
        );
        assert_eq!(label, "Steerer \u{b7} (removed #6)");
    }

    #[test]
    fn caller_key_and_label_unattributed_steerer_role() {
        let (key, label) = caller_key_and_label("steerer", None, None, None);
        assert_eq!(key, "steerer");
        assert_eq!(label, "Steerer (unattributed)");
    }

    #[test]
    fn caller_key_and_label_known_roles_map_to_display_names() {
        assert_eq!(
            caller_key_and_label("judge_worker", None, None, None),
            ("judge_worker".to_owned(), "Turn judge".to_owned())
        );
        assert_eq!(
            caller_key_and_label("session_judge", None, None, None),
            ("session_judge".to_owned(), "Session judge".to_owned())
        );
        assert_eq!(
            caller_key_and_label("eval", None, None, None),
            ("eval".to_owned(), "Eval runs".to_owned())
        );
        assert_eq!(
            caller_key_and_label("lesson_relations", None, None, None),
            ("lesson_relations".to_owned(), "Lesson relations".to_owned())
        );
    }

    #[test]
    fn caller_key_and_label_unknown_role_falls_back_to_role_name() {
        let (key, label) = caller_key_and_label("future_role", None, None, None);
        assert_eq!(key, "future_role");
        assert_eq!(label, "future_role");
    }
}
