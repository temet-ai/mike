use super::*;

#[derive(Debug, serde::Deserialize)]
pub(crate) struct DryRunRequest {
    pub(crate) session_id: String,
    pub(crate) prompt: String,
    pub(crate) anchor_request_id: Option<u64>,
    pub(crate) turn_limit: Option<u32>,
    pub(crate) confidence_threshold: Option<f64>,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct DryRunTurnResult {
    pub(crate) request_id: u64,
    pub(crate) turn_index: u32,
    pub(crate) user_prompt: String,
    pub(crate) would_fire: bool,
    pub(crate) confidence: f64,
    pub(crate) would_inject: String,
    pub(crate) violation_topic: String,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct DryRunResponse {
    pub(crate) turns: Vec<DryRunTurnResult>,
    pub(crate) would_fire_count: u32,
    pub(crate) total_evaluated: u32,
    pub(crate) model_used: String,
}

fn resolve_dry_run_llm(
    state: &AppState,
) -> Result<(crate::llm::OpenRouterClient, String), (StatusCode, Json<serde_json::Value>)> {
    match steerer_client(state, std::time::Duration::from_secs(15)) {
        Ok(SteererClient { llm, model_used }) => Ok((llm, model_used)),
        Err(message) => {
            tracing::warn!(%message, "steerer_dry_run: no usable LLM provider for the steerer model");
            Err((
                StatusCode::OK,
                Json(serde_json::json!({
                    "error": "dry_run_unavailable",
                    "message": message
                })),
            ))
        }
    }
}

async fn fetch_dry_run_candidate_turns(
    state: &AppState,
    session_id: &str,
    anchor_request_id: Option<u64>,
    turn_cap: usize,
) -> Vec<crate::store::persistence::operator_turns::SessionOverviewTurnRow> {
    let Some(handle) = state.persist() else {
        return Vec::new();
    };
    let db_id_opt = handle
        .resolve_session_db_id(session_id)
        .await
        .unwrap_or(None);
    let Some(db_id) = db_id_opt else {
        return Vec::new();
    };

    let rows = match handle
        .session_overview_bounded(
            db_id,
            SortOrder::Desc,
            &crate::store::persistence::operator_turns::SessionOverviewFilter::default(),
        )
        .await
    {
        Ok((rows, _, _)) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "steerer_dry_run: session_overview_bounded failed");
            return Vec::new();
        }
    };

    let filtered: Vec<_> = rows
        .into_iter()
        .filter(|t| {
            anchor_request_id
                .map(|anchor_id| {
                    t.stop_request_id
                        .map(|sid| sid as u64 >= anchor_id)
                        .unwrap_or(false)
                })
                .unwrap_or(true)
        })
        .take(turn_cap)
        .collect();
    let mut chron = filtered;
    chron.reverse();
    chron
}

async fn evaluate_dry_run_turns(
    llm: &crate::llm::OpenRouterClient,
    system_prompt: &str,
    schema: &serde_json::Value,
    draft_prompt: &str,
    confidence_threshold: f64,
    candidate_turns: &[crate::store::persistence::operator_turns::SessionOverviewTurnRow],
) -> Result<(Vec<DryRunTurnResult>, u32), (StatusCode, Json<serde_json::Value>)> {
    use crate::steerer::refocus::build_dry_run_user_message;
    use crate::steerer::verdict::{SteererVerdict, clears_confidence_threshold};

    let mut turn_results: Vec<DryRunTurnResult> = Vec::with_capacity(candidate_turns.len());
    let mut would_fire_count: u32 = 0;

    for (idx, turn) in candidate_turns.iter().enumerate() {
        let user_msg = turn.user_prompt.as_str();
        let user_prompt = build_dry_run_user_message(draft_prompt, user_msg);

        let verdict: SteererVerdict = match llm
            .verdict::<SteererVerdict>(system_prompt, &user_prompt, schema.clone())
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    turn_id = turn.turn_id,
                    "steerer_dry_run: verdict call failed for turn"
                );
                return Err((
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "error": "dry_run_unavailable",
                        "message": format!("LLM verdict call failed: {e}")
                    })),
                ));
            }
        };

        let would_fire =
            verdict.fires && clears_confidence_threshold(&verdict, confidence_threshold);

        let would_inject = if would_fire {
            verdict.nudge.clone()
        } else {
            String::new()
        };

        if would_fire {
            would_fire_count += 1;
        }

        turn_results.push(DryRunTurnResult {
            request_id: turn.stop_request_id.unwrap_or(turn.start_request_id) as u64,
            turn_index: (idx + 1) as u32,
            user_prompt: user_msg.to_owned(),
            would_fire,
            confidence: verdict.confidence,
            would_inject,
            violation_topic: verdict.reason,
        });
    }

    Ok((turn_results, would_fire_count))
}

pub(crate) async fn steerer_dry_run(
    State(state): State<AppState>,
    Json(body): Json<DryRunRequest>,
) -> Result<Json<DryRunResponse>, (StatusCode, Json<serde_json::Value>)> {
    use crate::steerer::refocus::resolve_verdict_system_prompt;
    use crate::steerer::verdict::steerer_verdict_schema;

    let (llm, model) = resolve_dry_run_llm(&state)?;

    let turn_cap = body.turn_limit.unwrap_or(5).min(20) as usize;
    let confidence_threshold = body.confidence_threshold.unwrap_or(0.7);

    let candidate_turns =
        fetch_dry_run_candidate_turns(&state, &body.session_id, body.anchor_request_id, turn_cap)
            .await;

    let system_prompt =
        resolve_verdict_system_prompt(state.persist(), 0, Some(body.prompt.as_str())).await;
    let schema = steerer_verdict_schema();

    let (turn_results, would_fire_count) = evaluate_dry_run_turns(
        &llm,
        system_prompt.as_str(),
        &schema,
        body.prompt.as_str(),
        confidence_threshold,
        &candidate_turns,
    )
    .await?;

    let total_evaluated = turn_results.len() as u32;
    Ok(Json(DryRunResponse {
        turns: turn_results,
        would_fire_count,
        total_evaluated,
        model_used: model,
    }))
}
