use super::*;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub(crate) struct AssistFinding {
    pub(crate) category: String,
    pub(crate) advice: String,
    pub(crate) matched_text: String,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct AssistRequest {
    pub(crate) session_id: String,
    pub(crate) user_intent: String,
    pub(crate) anchor_request_id: Option<u64>,
    pub(crate) anchor_findings: Option<Vec<AssistFinding>>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct AssistSuggestion {
    pub(crate) prompt: String,
    pub(crate) authority: String,
    pub(crate) mode: String,
    pub(crate) config: AssistSuggestionConfig,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct AssistSuggestionConfig {
    pub(crate) confidence_threshold: f64,
    pub(crate) debounce_turns: i32,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct AssistRationale {
    pub(crate) prompt: String,
    pub(crate) authority: String,
    pub(crate) mode: String,
    pub(crate) confidence_threshold: String,
    pub(crate) debounce_turns: String,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct AssistResponse {
    pub(crate) suggestion: AssistSuggestion,
    pub(crate) rationale: AssistRationale,
    pub(crate) model_used: String,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct AssistLlmOutput {
    pub(crate) suggestion: AssistSuggestion,
    pub(crate) rationale: AssistRationale,
}

type AssistError = (StatusCode, Json<serde_json::Value>);

fn assist_unavailable(message: String) -> AssistError {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "error": "assist_unavailable",
            "message": message
        })),
    )
}

pub(crate) async fn steerer_assist(
    State(state): State<AppState>,
    Json(body): Json<AssistRequest>,
) -> Result<Json<AssistResponse>, AssistError> {
    let SteererClient { llm, model_used } = steerer_client(
        &state,
        std::time::Duration::from_secs(15),
    )
    .map_err(|message| {
        tracing::warn!(%message, "steerer_assist: no usable LLM provider for the steerer model");
        assist_unavailable(message)
    })?;

    let mut session_context = build_session_context(&state, &body.session_id).await;
    annotate_anchor_request(&mut session_context, body.anchor_request_id);

    let system_prompt =
        crate::steerer::steer_author::resolve_steer_author_system_prompt(state.persist()).await;

    let findings_context = build_findings_context(body.anchor_findings.as_deref());
    let user_prompt = build_user_prompt(&session_context, &findings_context, &body.user_intent);

    run_assist_verdict(&llm, &system_prompt, &user_prompt, model_used).await
}

async fn build_session_context(state: &AppState, session_id: &str) -> String {
    let Some(handle) = state.persist() else {
        return String::from("[persistence disabled]");
    };
    let db_id_opt = handle
        .resolve_session_db_id(session_id)
        .await
        .unwrap_or(None);
    let Some(db_id) = db_id_opt else {
        return String::from("[session not found]");
    };
    match handle
        .session_overview_bounded(
            db_id,
            SortOrder::Desc,
            &crate::store::persistence::operator_turns::SessionOverviewFilter::default(),
        )
        .await
    {
        Ok((rows, _, _)) => format_recent_turns(rows),
        Err(_) => String::from("[session context unavailable]"),
    }
}

fn format_recent_turns(
    rows: Vec<crate::store::persistence::operator_turns::SessionOverviewTurnRow>,
) -> String {
    let recent: Vec<_> = rows.into_iter().take(5).collect();
    let mut turns_text = String::new();
    for (i, t) in recent.iter().rev().enumerate() {
        turns_text.push_str(&format!(
            "Turn {}:\nUser: {}\nAssistant: {}\n\n",
            i + 1,
            t.user_prompt,
            t.stop_response.as_deref().unwrap_or("[no response]")
        ));
    }
    turns_text
}

fn annotate_anchor_request(session_context: &mut String, anchor_request_id: Option<u64>) {
    if let Some(req_id) = anchor_request_id {
        session_context.push_str(&format!(
            "\n[Operator is targeting response #{req_id} as the anchor point for this steer.]\n"
        ));
    }
}

fn build_findings_context(findings: Option<&[AssistFinding]>) -> String {
    match findings {
        Some(findings) if !findings.is_empty() => {
            let mut s = String::from("\nDetected behavioral violations at the anchor response:\n");
            for f in findings {
                s.push_str(&format!(
                    "- [{}] {} (matched: \"{}\")\n",
                    f.category, f.advice, f.matched_text
                ));
            }
            s.push_str("\nThe constraint MUST explicitly forbid these behaviors.\n");
            s
        }
        _ => String::new(),
    }
}

fn build_user_prompt(session_context: &str, findings_context: &str, user_intent: &str) -> String {
    format!(
        "Recent session context (last 5 turns):\n{session_context}\
         {findings_context}\n\
         What the operator wants to correct: {user_intent}\n\n\
         Author the steering anchor for this steer."
    )
}

fn assist_llm_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "suggestion": {
                "type": "object",
                "properties": {
                    "prompt": {"type": "string"},
                    "authority": {"type": "string"},
                    "mode": {"type": "string"},
                    "config": {
                        "type": "object",
                        "properties": {
                            "confidence_threshold": {"type": "number"},
                            "debounce_turns": {"type": "integer"}
                        },
                        "required": ["confidence_threshold", "debounce_turns"],
                        "additionalProperties": false
                    }
                },
                "required": ["prompt", "authority", "mode", "config"],
                "additionalProperties": false
            },
            "rationale": {
                "type": "object",
                "properties": {
                    "prompt": {"type": "string"},
                    "authority": {"type": "string"},
                    "mode": {"type": "string"},
                    "confidence_threshold": {"type": "string"},
                    "debounce_turns": {"type": "string"}
                },
                "required": ["prompt", "authority", "mode", "confidence_threshold", "debounce_turns"],
                "additionalProperties": false
            }
        },
        "required": ["suggestion", "rationale"],
        "additionalProperties": false
    })
}

async fn run_assist_verdict(
    llm: &crate::llm::OpenRouterClient,
    system_prompt: &str,
    user_prompt: &str,
    model: String,
) -> Result<Json<AssistResponse>, AssistError> {
    match llm
        .verdict::<AssistLlmOutput>(system_prompt, user_prompt, assist_llm_schema())
        .await
    {
        Ok(output) => Ok(Json(AssistResponse {
            suggestion: output.suggestion,
            rationale: output.rationale,
            model_used: model,
        })),
        Err(e) => {
            tracing::warn!(error = %e, "steerer_assist: LLM call failed");
            Err(assist_unavailable(format!("LLM call failed: {e}")))
        }
    }
}
