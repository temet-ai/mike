use super::*;

async fn resolve_session_scan_ids(
    handle: &crate::store::persistence::PersistHandle,
    session_id: &str,
    caller: &'static str,
) -> Result<
    (
        i64,
        Vec<crate::store::persistence::rehydrate::SessionRequestIdRow>,
    ),
    (StatusCode, Json<FindTextErrorBody>),
> {
    let session_db_id = match handle.resolve_session_db_id(session_id).await {
        Ok(Some(db_id)) => db_id,
        Ok(None) => {
            return Err(find_text_error(
                StatusCode::NOT_FOUND,
                &format!("session id {session_id} not found"),
            ));
        }
        Err(e) => {
            tracing::warn!(error = %e, session_id = %session_id, caller, "find_text: session resolve failed");
            return Err(find_text_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "lookup failed",
            ));
        }
    };

    let request_ids = handle
        .list_session_request_ids_asc(session_db_id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, session_id = %session_id, caller, "find_text: request id list failed");
            find_text_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed")
        })?;

    Ok((session_db_id, request_ids))
}

fn scoped_session_response(
    params: &FindTextMatchParams<'_>,
    scan: SessionScan,
    cursor: Option<u64>,
    requests_total: usize,
    requests_skipped: usize,
) -> FindTextUnifiedResponse {
    let complete = scan.complete;
    let acc = scan.acc;
    FindTextUnifiedResponse {
        needle: params.needle.to_owned(),
        case_sensitive: params.case_sensitive,
        context_bytes: params.context_bytes,
        positions: if params.reads_messages() {
            TextPositions::Message
        } else {
            TextPositions::Body
        },
        full_match_count: complete.then_some(acc.full_count),
        matches_truncated: acc.matches.len() < acc.full_count as usize,
        matches: acc.matches,
        window_note: None,
        next_cursor: if complete {
            None
        } else {
            scan.last_scanned.or(cursor)
        },
        has_more: !complete,
        scan_complete: Some(complete),
        requests_scanned: Some((requests_skipped + scan.scanned) as u64),
        requests_total: Some(requests_total as u64),
        reminder_matches_skipped: params.reads_messages().then_some(acc.reminder_skipped),
        own_search_matches_skipped: (params.reads_messages()
            || params.source == FindTextSource::Response)
            .then_some(acc.own_skipped),
    }
}

pub(crate) async fn find_text_scoped_session(
    state: &AppState,
    session_id: &str,
    params: &FindTextMatchParams<'_>,
    cursor: Option<u64>,
    limits: &ScanLimits,
) -> Result<Json<FindTextUnifiedResponse>, (StatusCode, Json<FindTextErrorBody>)> {
    if uuid::Uuid::parse_str(session_id).is_err() {
        return Err(find_text_error(StatusCode::NOT_FOUND, "unknown session"));
    }

    let Some(handle) = state.persist() else {
        return Err(find_text_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence unavailable",
        ));
    };

    let (session_db_id, request_ids) =
        resolve_session_scan_ids(handle, session_id, "find_text_scoped_session").await?;

    let pending: Vec<i64> = request_ids
        .iter()
        .map(|r| r.request_id)
        .filter(|id| cursor.is_none_or(|c| (*id as u64) > c))
        .collect();
    let ctx = ScanCtx {
        state,
        handle,
        session_db_id,
        session_id,
        params,
    };
    let scan = scan_session(&ctx, &pending, limits).await?;

    Ok(Json(scoped_session_response(
        params,
        scan,
        cursor,
        request_ids.len(),
        request_ids.len() - pending.len(),
    )))
}

#[cfg(test)]
#[path = "search_session_support_tests.rs"]
pub(crate) mod search_session_test_support;

#[cfg(test)]
#[path = "search_session_tests.rs"]
mod search_session_tests;

#[derive(Debug, Serialize)]
pub(crate) struct SessionTextMatch {
    pub(crate) request_id: u64,
    pub(crate) source: &'static str,
    pub(crate) byte_offset: u64,
    pub(crate) match_length: u32,
    pub(crate) line: u32,
    pub(crate) column: u32,
    pub(crate) before: String,
    pub(crate) matched: String,
    pub(crate) after: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct FindTextInSessionResponse {
    pub(crate) session_id: String,
    pub(crate) needle: String,
    pub(crate) case_sensitive: bool,
    pub(crate) context_bytes: u32,
    pub(crate) full_match_count: u64,
    pub(crate) matches_truncated: bool,
    pub(crate) matches: Vec<SessionTextMatch>,
}

async fn scan_session_for_matches(
    state: &AppState,
    request_ids: &[crate::store::persistence::rehydrate::SessionRequestIdRow],
    params: &FindTextParams,
    context_bytes: u32,
    max_matches: u32,
) -> (Vec<SessionTextMatch>, u64) {
    let mut all_matches: Vec<SessionTextMatch> = Vec::new();
    let mut full_count: u64 = 0;
    let remaining_cap = max_matches as usize;

    'outer: for id_row in request_ids {
        let request_id = id_row.request_id as u64;
        let outer_body = match resolve_body_for_find(state, request_id, params.source).await {
            Ok(Some(body)) => body,
            Ok(None) => continue,
            Err(_) => continue,
        };
        let text = match outer_body
            .as_ref()
            .and_then(|b| std::str::from_utf8(b).ok())
        {
            Some(s) => s.to_string(),
            None => continue,
        };
        let remaining = (remaining_cap.saturating_sub(all_matches.len())) as u32;
        let (count_this, new_matches) = crate::web_base::body_search::find_in_str(
            &text,
            &params.needle,
            params.case_sensitive,
            context_bytes,
            remaining,
        );
        full_count += count_this;
        let source_str = params.source.as_wire_str();
        for m in new_matches {
            if all_matches.len() >= remaining_cap {
                break 'outer;
            }
            all_matches.push(SessionTextMatch {
                request_id,
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

    (all_matches, full_count)
}

pub(crate) async fn find_text_in_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<FindTextParams>,
) -> Result<Json<FindTextInSessionResponse>, (StatusCode, Json<FindTextErrorBody>)> {
    if uuid::Uuid::parse_str(&id).is_err() {
        return Err(find_text_error(StatusCode::NOT_FOUND, "unknown session"));
    }
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

    let Some(handle) = state.persist() else {
        return Err(find_text_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence unavailable",
        ));
    };

    let (_session_db_id, request_ids) =
        resolve_session_scan_ids(handle, &id, "find_text_in_session").await?;

    let (all_matches, full_count) =
        scan_session_for_matches(&state, &request_ids, &params, context_bytes, max_matches).await;

    let truncated = all_matches.len() < full_count as usize;

    Ok(Json(FindTextInSessionResponse {
        session_id: id,
        needle: params.needle,
        case_sensitive: params.case_sensitive,
        context_bytes,
        full_match_count: full_count,
        matches_truncated: truncated,
        matches: all_matches,
    }))
}
