use super::*;

pub(crate) struct FindTextMatchParams<'a> {
    pub(crate) needle: &'a str,
    pub(crate) source: FindTextSource,
    pub(crate) case_sensitive: bool,
    pub(crate) context_bytes: u32,
    pub(crate) max_matches: u32,
}

pub(crate) async fn find_text(
    State(state): State<AppState>,
    Query(params): Query<FindTextUnifiedParams>,
) -> Result<Json<FindTextUnifiedResponse>, (StatusCode, Json<FindTextErrorBody>)> {
    if params.needle.is_empty() {
        return Err(find_text_error(
            StatusCode::BAD_REQUEST,
            "needle is required",
        ));
    }
    if params.needle.len() > FIND_TEXT_MAX_NEEDLE_BYTES {
        return Err(find_text_error(
            StatusCode::BAD_REQUEST,
            &format!(
                "needle exceeds {} bytes (got {})",
                FIND_TEXT_MAX_NEEDLE_BYTES,
                params.needle.len()
            ),
        ));
    }

    let context_bytes = params
        .context_bytes
        .unwrap_or(FIND_TEXT_DEFAULT_CONTEXT_BYTES)
        .min(FIND_TEXT_MAX_CONTEXT_BYTES);
    let max_matches = params
        .max_matches
        .unwrap_or(FIND_TEXT_DEFAULT_MAX_MATCHES)
        .clamp(1, FIND_TEXT_MAX_MAX_MATCHES);

    match (params.request_id, params.session_id.as_deref()) {
        (Some(rid), _) => {
            let match_params = FindTextMatchParams {
                needle: &params.needle,
                source: params.source,
                case_sensitive: params.case_sensitive,
                context_bytes,
                max_matches,
            };
            find_text_scoped_request(&state, rid, &match_params).await
        }
        (None, Some(sid)) => {
            let match_params = FindTextMatchParams {
                needle: &params.needle,
                source: params.source,
                case_sensitive: params.case_sensitive,
                context_bytes,
                max_matches,
            };
            find_text_scoped_session(&state, sid, &match_params, params.cursor).await
        }
        (None, None) => {
            if params.source != FindTextSource::Response {
                return Err(find_text_error(
                    StatusCode::BAD_REQUEST,
                    "unscoped search is response-only; pass request_id or session_id for \
                     body search, or use recall_similar_turns to find a session by content",
                ));
            }
            find_text_unscoped_corpus(&state, &params, context_bytes, max_matches).await
        }
    }
}

async fn find_text_unscoped_corpus(
    state: &AppState,
    params: &FindTextUnifiedParams,
    context_bytes: u32,
    max_matches: u32,
) -> Result<Json<FindTextUnifiedResponse>, (StatusCode, Json<FindTextErrorBody>)> {
    let Some(handle) = state.persist() else {
        return Err(find_text_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence unavailable",
        ));
    };
    let has_id_scope = false;
    let window = crate::api::search_window::resolve_window(
        params.date_from.as_deref(),
        params.date_to.as_deref(),
        has_id_scope,
    )
    .map_err(|e| find_text_error(StatusCode::BAD_REQUEST, &format!("invalid date: {e}")))?;

    let from = window
        .effective_from
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(7));
    let to = window.effective_to.unwrap_or_else(chrono::Utc::now);

    let (full_count, all_matches) = tokio::time::timeout(
        std::time::Duration::from_secs(FIND_TEXT_CORPUS_TIMEOUT_SECS),
        scan_corpus_candidates(handle, params, context_bytes, max_matches, from, to),
    )
    .await
    .map_err(|_| {
        find_text_error(
            StatusCode::GATEWAY_TIMEOUT,
            "corpus scan timed out — narrow the range with date_from/date_to \
             or provide a session_id",
        )
    })??;

    let truncated = all_matches.len() < full_count as usize;
    let window_note = if window.defaulted || window.windowed || all_matches.is_empty() {
        Some(window.widen_hint.clone())
    } else {
        None
    };

    Ok(Json(FindTextUnifiedResponse {
        needle: params.needle.clone(),
        case_sensitive: params.case_sensitive,
        context_bytes,
        full_match_count: Some(full_count),
        matches_truncated: truncated,
        matches: all_matches,
        window_note,
        next_cursor: None,
        has_more: false,
    }))
}

async fn scan_corpus_candidates(
    handle: &crate::store::persistence::PersistHandle,
    params: &FindTextUnifiedParams,
    context_bytes: u32,
    max_matches: u32,
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
) -> Result<(u64, Vec<UnifiedTextMatch>), (StatusCode, Json<FindTextErrorBody>)> {
    let body_source = params.source.to_body_source();
    let candidates = crate::store::persistence::rehydrate::find_text_candidates(
        handle.pool(),
        params.needle.as_bytes(),
        crate::store::persistence::rehydrate::FindTextQuery {
            source: body_source,
            from,
            to,
            candidate_cap: FIND_TEXT_CORPUS_CANDIDATE_CAP,
        },
    )
    .await
    .map_err(|e| {
        tracing::warn!(error = %e, "find_text: corpus candidate query failed");
        find_text_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed")
    })?;

    let source_str = params.source.as_wire_str();
    let remaining_cap = max_matches as usize;
    let mut all_matches: Vec<UnifiedTextMatch> = Vec::new();
    let mut full_count: u64 = 0;

    let mut sorted_candidates = candidates;
    sorted_candidates.sort_unstable_by_key(|c| std::cmp::Reverse(c.created_at));

    let ids: Vec<i64> = sorted_candidates.iter().map(|c| c.request_id).collect();
    let bodies = handle
        .fetch_response_bodies_batch(&ids)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "find_text: batched body fetch failed");
            find_text_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed")
        })?;

    for candidate in &sorted_candidates {
        let Some(body) = bodies.get(&candidate.request_id) else {
            continue;
        };
        let Some(text) = std::str::from_utf8(body).ok() else {
            continue;
        };
        let (count_this, new_matches) = crate::api::body_search::find_in_str(
            text,
            &params.needle,
            params.case_sensitive,
            context_bytes,
            max_matches,
        );
        full_count += count_this;
        for m in new_matches {
            if all_matches.len() >= remaining_cap {
                continue;
            }
            all_matches.push(UnifiedTextMatch {
                request_id: candidate.request_id as u64,
                session_id: candidate.session_id.clone(),
                source: source_str,
                byte_offset: m.byte_offset,
                match_length: m.match_length,
                line: m.line,
                column: m.column,
                before: m.before,
                matched: m.matched,
                after: m.after,
            });
        }
    }

    Ok((full_count, all_matches))
}

pub(crate) async fn find_text_scoped_request(
    state: &AppState,
    rid: u64,
    params: &FindTextMatchParams<'_>,
) -> Result<Json<FindTextUnifiedResponse>, (StatusCode, Json<FindTextErrorBody>)> {
    let body = match resolve_body_for_find(state, rid, params.source).await {
        Ok(Some(body)) => body,
        Ok(None) => {
            return Err(find_text_error(
                StatusCode::NOT_FOUND,
                &format!("request id {rid} not found"),
            ));
        }
        Err(status) => {
            return Err(find_text_error(status, "body lookup failed"));
        }
    };

    let session_id = match state.persist() {
        Some(handle) => match handle.fetch_request_by_id(rid).await {
            Ok(Some(req)) => req.session_id,
            Ok(None) => {
                return Err(find_text_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "session metadata unavailable for request",
                ));
            }
            Err(e) => {
                tracing::warn!(error = %e, request_id = rid, "find_text: session lookup failed");
                return Err(find_text_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "session lookup failed",
                ));
            }
        },
        None => {
            return Err(find_text_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "persistence unavailable — cannot resolve session_id for request",
            ));
        }
    };

    let source_str = params.source.as_wire_str();
    let text = match body.as_ref().and_then(|b| std::str::from_utf8(b).ok()) {
        Some(s) => s.to_string(),
        None => {
            return Ok(Json(FindTextUnifiedResponse {
                needle: params.needle.to_owned(),
                case_sensitive: params.case_sensitive,
                context_bytes: params.context_bytes,
                full_match_count: Some(0),
                matches_truncated: false,
                matches: Vec::new(),
                window_note: None,
                next_cursor: None,
                has_more: false,
            }));
        }
    };

    let (full_count, raw_matches) = crate::api::body_search::find_in_str(
        &text,
        params.needle,
        params.case_sensitive,
        params.context_bytes,
        params.max_matches,
    );
    let truncated = full_count > raw_matches.len() as u64;

    let matches = raw_matches
        .into_iter()
        .map(|m| UnifiedTextMatch {
            request_id: rid,
            session_id: session_id.clone(),
            source: source_str,
            byte_offset: m.byte_offset,
            match_length: m.match_length,
            line: m.line,
            column: m.column,
            before: m.before,
            matched: m.matched,
            after: m.after,
        })
        .collect();

    Ok(Json(FindTextUnifiedResponse {
        needle: params.needle.to_owned(),
        case_sensitive: params.case_sensitive,
        context_bytes: params.context_bytes,
        full_match_count: Some(full_count),
        matches_truncated: truncated,
        matches,
        window_note: None,
        next_cursor: None,
        has_more: false,
    }))
}
