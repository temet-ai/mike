use super::*;

pub(crate) async fn dry_run_filter_rules(
    State(state): State<AppState>,
    Json(body): Json<FilterDryRunRequest>,
) -> Result<Json<FilterDryRunResponse>, (StatusCode, Json<FilterRuleErrorBody>)> {
    let cfg = build_dry_run_ruleset(&state, body.rules)?;
    let raw = load_dry_run_request_body(&state, body.request_id).await?;

    let project_path: String = match serde_json::from_slice::<serde_json::Value>(&raw) {
        Ok(v) => {
            let sp = crate::extract::request::extract_system_prompt(&v);
            let message_sources = crate::extract::request::collect_project_scan_sources(&v);
            crate::extract::request::extract_project(crate::extract::request::ProjectSignals {
                system_prompt: sp.as_deref(),
                message_sources,
            })
            .unwrap_or_default()
        }
        Err(_) => String::new(),
    };

    let outcome = crate::filter::apply_request_filter(&raw, &cfg, &project_path);
    let (changed, firing_rule_ids, filtered_bytes, original_len, filtered_len) = match outcome {
        crate::filter::FilterOutcome::Unchanged => {
            (false, Vec::new(), raw.clone(), raw.len(), raw.len())
        }
        crate::filter::FilterOutcome::Rewrote {
            new_body,
            original_len,
            filtered_len,
            firing_rule_ids,
        } => (true, firing_rule_ids, new_body, original_len, filtered_len),
    };

    let (filtered_system, filtered_messages) =
        match serde_json::from_slice::<serde_json::Value>(&filtered_bytes) {
            Ok(mut v) => match v.as_object_mut() {
                Some(obj) => (obj.remove("system"), obj.remove("messages")),
                None => (None, None),
            },
            Err(_) => (None, None),
        };

    Ok(Json(FilterDryRunResponse {
        request_id: body.request_id,
        changed,
        project_path,
        firing_rule_ids,
        original_len,
        filtered_len,
        filtered_system,
        filtered_messages,
    }))
}

fn build_dry_run_ruleset(
    state: &AppState,
    drafts: Option<Vec<FilterRuleCreate>>,
) -> Result<crate::filter::FilterConfig, (StatusCode, Json<FilterRuleErrorBody>)> {
    let Some(drafts) = drafts else {
        return Ok((*state.filter_config.load_full()).clone());
    };
    let mut rules = Vec::with_capacity(drafts.len());
    for (idx, draft) in drafts.into_iter().enumerate() {
        let positional = idx + 1;
        let kind = crate::filter::service::parse_kind(&draft.kind).map_err(|msg| {
            filter_error_response(
                StatusCode::BAD_REQUEST,
                &format!("draft rule {positional}: {msg}"),
            )
        })?;
        let action = crate::filter::service::parse_action(&draft.action).map_err(|msg| {
            filter_error_response(
                StatusCode::BAD_REQUEST,
                &format!("draft rule {positional}: {msg}"),
            )
        })?;
        let rule_id = draft
            .id
            .as_deref()
            .and_then(|id| id.parse::<i64>().ok())
            .unwrap_or(-(positional as i64));
        let rule = crate::filter::FilterRule {
            id: rule_id,
            uuid: uuid::Uuid::new_v4(),
            kind,
            value: draft.value,
            enabled: draft.enabled,
            source: crate::filter::RuleSource::Ui,
            updated_at: chrono::Utc::now(),
            sort_order: idx as i32,
            action,
            text: draft.text,
            trigger_token: draft.trigger_token,
            inject_text: draft.inject_text,
            project_path: draft
                .project_path
                .as_deref()
                .map(crate::filter::canonicalize_project_path),
            style_override: draft.style_override,
            title: crate::filter::normalize_title(draft.title),
        };
        rule.validate_structure()
            .map_err(|msg| filter_error_response(StatusCode::BAD_REQUEST, &msg))?;
        rules.push(rule);
    }
    Ok(crate::filter::FilterConfig { rules })
}

async fn load_dry_run_request_body(
    state: &AppState,
    request_id: u64,
) -> Result<Bytes, (StatusCode, Json<FilterRuleErrorBody>)> {
    let in_memory = {
        let store = state.store.read().await;
        store.get(&RequestId::from_raw(request_id)).cloned()
    };
    if let Some(req) = in_memory {
        return req.request_body.clone().ok_or_else(|| {
            filter_error_response(
                StatusCode::NOT_FOUND,
                "request has no captured body to dry-run against",
            )
        });
    }
    match load_request_body_from_db(state, request_id).await {
        Ok(Some((Some(b), _reconstructed))) => Ok(b),
        Ok(Some((None, _))) => Err(filter_error_response(
            StatusCode::NOT_FOUND,
            "request has no captured body to dry-run against",
        )),
        Ok(None) => Err(filter_error_response(
            StatusCode::NOT_FOUND,
            "no request with that id",
        )),
        Err(status) => Err(filter_error_response(
            status,
            "failed to load the captured request body",
        )),
    }
}
