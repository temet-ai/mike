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

async fn fetch_scan_batch_bodies(
    handle: &crate::store::persistence::PersistHandle,
    source: FindTextSource,
    page_ids: &[i64],
    session_id: &str,
) -> Result<std::collections::HashMap<i64, Bytes>, (StatusCode, Json<FindTextErrorBody>)> {
    match source {
        FindTextSource::Response => handle
            .fetch_response_bodies_batch(page_ids)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, session_id = %session_id, "find_text: batched response body fetch failed");
                find_text_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed")
            }),
        FindTextSource::Request => handle
            .fetch_request_bodies_batch(page_ids)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, session_id = %session_id, "find_text: batched request body fetch failed");
                find_text_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed")
            }),
        FindTextSource::RequestSent => Ok(std::collections::HashMap::new()),
    }
}

async fn scan_session_page(
    state: &AppState,
    session_id: &str,
    scan_slice: &[&crate::store::persistence::rehydrate::SessionRequestIdRow],
    batched_bodies: &std::collections::HashMap<i64, Bytes>,
    params: &FindTextMatchParams<'_>,
) -> (Vec<UnifiedTextMatch>, u64, Option<u64>) {
    let source_str = params.source.as_wire_str();
    let remaining_cap = params.max_matches as usize;
    let mut all_matches: Vec<UnifiedTextMatch> = Vec::new();
    let mut full_count: u64 = 0;
    let mut last_scanned: Option<u64> = None;

    for id_row in scan_slice {
        let rid = id_row.request_id as u64;
        last_scanned = Some(rid);
        let body = if matches!(params.source, FindTextSource::RequestSent) {
            resolve_body_for_find(state, rid, params.source)
                .await
                .ok()
                .flatten()
                .flatten()
        } else {
            batched_bodies.get(&id_row.request_id).cloned()
        };
        let Some(body) = body else {
            continue;
        };
        let Some(text) = std::str::from_utf8(&body).ok() else {
            continue;
        };
        let (count_this, new_matches) = crate::api::body_search::find_in_str(
            text,
            params.needle,
            params.case_sensitive,
            params.context_bytes,
            params.max_matches,
        );
        full_count += count_this;
        for m in new_matches {
            if all_matches.len() >= remaining_cap {
                continue;
            }
            all_matches.push(UnifiedTextMatch {
                request_id: rid,
                session_id: session_id.to_owned(),
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

    (all_matches, full_count, last_scanned)
}

pub(crate) async fn find_text_scoped_session(
    state: &AppState,
    session_id: &str,
    params: &FindTextMatchParams<'_>,
    cursor: Option<u64>,
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

    let (_session_db_id, request_ids) =
        resolve_session_scan_ids(handle, session_id, "find_text_scoped_session").await?;

    let scan_slice: Vec<_> = request_ids
        .iter()
        .filter(|r| cursor.is_none_or(|c| (r.request_id as u64) > c))
        .take(FIND_TEXT_SESSION_PAGE_REQUESTS)
        .collect();

    let page_ids: Vec<i64> = scan_slice.iter().map(|r| r.request_id).collect();
    let batched_bodies =
        fetch_scan_batch_bodies(handle, params.source, &page_ids, session_id).await?;

    let (all_matches, full_count, last_scanned) =
        scan_session_page(state, session_id, &scan_slice, &batched_bodies, params).await;

    let scan_boundary = last_scanned.or(cursor);
    let has_more = match scan_boundary {
        Some(boundary) => request_ids.iter().any(|r| (r.request_id as u64) > boundary),
        None => false,
    };
    let next_cursor = if has_more { scan_boundary } else { None };

    let truncated = all_matches.len() < full_count as usize;

    let full_match_count = if has_more { None } else { Some(full_count) };

    Ok(Json(FindTextUnifiedResponse {
        needle: params.needle.to_owned(),
        case_sensitive: params.case_sensitive,
        context_bytes: params.context_bytes,
        full_match_count,
        matches_truncated: truncated,
        matches: all_matches,
        window_note: None,
        next_cursor,
        has_more,
    }))
}

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
        let (count_this, new_matches) = crate::api::body_search::find_in_str(
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
