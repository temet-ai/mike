use super::*;

pub(crate) const FIND_TEXT_MAX_CONTEXT_BYTES: u32 = 4096;
pub(crate) const FIND_TEXT_DEFAULT_MAX_MATCHES: u32 = 50;
pub(crate) const FIND_TEXT_MAX_MAX_MATCHES: u32 = 500;
pub(crate) const FIND_TEXT_MAX_NEEDLE_BYTES: usize = 8192;

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FindTextSource {
    #[default]
    Request,
    RequestSent,
    Response,
}

impl FindTextSource {
    pub(crate) fn as_wire_str(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::RequestSent => "request_sent",
            Self::Response => "response",
        }
    }

    pub(crate) fn to_body_source(self) -> crate::store::persistence::rehydrate::BodySource {
        use crate::store::persistence::rehydrate::BodySource;
        match self {
            Self::Request => BodySource::Request,
            Self::RequestSent => BodySource::RequestSent,
            Self::Response => BodySource::Response,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct FindTextParams {
    pub(crate) needle: String,
    #[serde(default)]
    pub(crate) source: FindTextSource,
    #[serde(default = "default_find_text_case_sensitive")]
    pub(crate) case_sensitive: bool,
    #[serde(default)]
    pub(crate) context_bytes: Option<u32>,
    #[serde(default)]
    pub(crate) max_matches: Option<u32>,
}

pub(crate) fn default_find_text_case_sensitive() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub(crate) struct FindTextResponse {
    pub(crate) source: &'static str,
    pub(crate) byte_size: u64,
    pub(crate) needle: String,
    pub(crate) case_sensitive: bool,
    pub(crate) context_bytes: u32,
    pub(crate) full_match_count: u64,
    pub(crate) matches_truncated: bool,
    pub(crate) matches: Vec<crate::api::body_search::TextMatch>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FindTextErrorBody {
    pub(crate) error: String,
}

pub(crate) async fn find_text_in_body(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Query(params): Query<FindTextParams>,
) -> Result<Json<FindTextResponse>, (StatusCode, Json<FindTextErrorBody>)> {
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

    let body = match resolve_body_for_find(&state, id, params.source).await {
        Ok(Some(body)) => body,
        Ok(None) => {
            return Err(find_text_error(
                StatusCode::NOT_FOUND,
                &format!("request id {id} not found"),
            ));
        }
        Err(status) => {
            return Err(find_text_error(status, "body lookup failed"));
        }
    };

    let byte_size = byte_size(body.as_ref());
    let text = match body.as_ref().and_then(|b| std::str::from_utf8(b).ok()) {
        Some(s) => s.to_string(),
        None => {
            return Ok(Json(FindTextResponse {
                source: params.source.as_wire_str(),
                byte_size,
                needle: params.needle,
                case_sensitive: params.case_sensitive,
                context_bytes,
                full_match_count: 0,
                matches_truncated: false,
                matches: Vec::new(),
            }));
        }
    };

    let (full_count, matches) = crate::api::body_search::find_in_str(
        &text,
        &params.needle,
        params.case_sensitive,
        context_bytes,
        max_matches,
    );
    let truncated = full_count > matches.len() as u64;

    Ok(Json(FindTextResponse {
        source: params.source.as_wire_str(),
        byte_size,
        needle: params.needle,
        case_sensitive: params.case_sensitive,
        context_bytes,
        full_match_count: full_count,
        matches_truncated: truncated,
        matches,
    }))
}

pub(crate) async fn resolve_body_for_find(
    state: &AppState,
    id: u64,
    source: FindTextSource,
) -> Result<Option<Option<Bytes>>, StatusCode> {
    match source {
        FindTextSource::Request => match load_request_body_from_db(state, id).await? {
            Some((bytes, _reconstructed)) => Ok(Some(bytes)),
            None => Ok(None),
        },
        FindTextSource::RequestSent => resolve_request_sent_body(state, id).await,
        FindTextSource::Response => match load_response_body_from_db(state, id)
            .await
            .map_err(|r| r.status())?
        {
            Some(hit) => Ok(Some(hit)),
            None => Ok(None),
        },
    }
}

pub(crate) fn find_text_error(
    status: StatusCode,
    error: &str,
) -> (StatusCode, Json<FindTextErrorBody>) {
    (
        status,
        Json(FindTextErrorBody {
            error: error.to_string(),
        }),
    )
}

#[derive(Debug, Deserialize)]
pub(crate) struct FindTextUnifiedParams {
    pub(crate) needle: String,
    #[serde(default)]
    pub(crate) request_id: Option<u64>,
    #[serde(default)]
    pub(crate) session_id: Option<String>,
    #[serde(default)]
    pub(crate) source: FindTextSource,
    #[serde(default = "default_find_text_case_sensitive")]
    pub(crate) case_sensitive: bool,
    #[serde(default)]
    pub(crate) context_bytes: Option<u32>,
    #[serde(default)]
    pub(crate) max_matches: Option<u32>,
    #[serde(default)]
    pub(crate) date_from: Option<String>,
    #[serde(default)]
    pub(crate) date_to: Option<String>,
    #[serde(default)]
    pub(crate) cursor: Option<u64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct UnifiedTextMatch {
    pub(crate) request_id: u64,
    pub(crate) session_id: String,
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
pub(crate) struct FindTextUnifiedResponse {
    pub(crate) needle: String,
    pub(crate) case_sensitive: bool,
    pub(crate) context_bytes: u32,
    pub(crate) full_match_count: Option<u64>,
    pub(crate) matches_truncated: bool,
    pub(crate) matches: Vec<UnifiedTextMatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) window_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) next_cursor: Option<u64>,
    #[serde(default)]
    pub(crate) has_more: bool,
}

pub(crate) const FIND_TEXT_CORPUS_TIMEOUT_SECS: u64 = 25;

pub(crate) const FIND_TEXT_CORPUS_CANDIDATE_CAP: i64 = 200;

pub(crate) const FIND_TEXT_SESSION_PAGE_REQUESTS: usize = 60;

pub(crate) const FIND_TEXT_DEFAULT_CONTEXT_BYTES: u32 = 256;
