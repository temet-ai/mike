use super::*;

pub(crate) fn window_upper_bound(
    window: &crate::web_base::search_window::SearchWindow,
) -> Option<chrono::DateTime<chrono::Utc>> {
    if window.defaulted {
        None
    } else {
        window.effective_to
    }
}

pub(crate) fn window_note_headers(
    window: &crate::web_base::search_window::SearchWindow,
) -> axum::http::HeaderMap {
    let mut headers = axum::http::HeaderMap::new();
    if window.windowed {
        let ascii: String = window
            .widen_hint
            .chars()
            .map(|c| if c.is_ascii() { c } else { '-' })
            .collect();
        if let Ok(v) = axum::http::HeaderValue::try_from(ascii) {
            headers.insert("x-window-note", v);
        }
    }
    headers
}

#[derive(Deserialize)]
pub(crate) struct ListParams {
    pub(crate) offset: Option<usize>,
    pub(crate) limit: Option<usize>,
    pub(crate) order: Option<SortOrder>,
    pub(crate) prompt_sha256: Option<String>,
    #[serde(default)]
    pub(crate) cursor: Option<String>,
    pub(crate) search: Option<String>,
    #[serde(default)]
    pub(crate) date_from: Option<String>,
    #[serde(default)]
    pub(crate) date_to: Option<String>,
    #[serde(default)]
    pub(crate) min_request_count: Option<i64>,
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) fields: Option<String>,
    #[serde(default)]
    pub(crate) ids: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SessionDetailParams {
    pub(crate) offset: Option<usize>,
    pub(crate) limit: Option<usize>,
    pub(crate) order: Option<SortOrder>,
}

pub(crate) const SESSION_DETAIL_DEFAULT_LIMIT: usize = 50;
pub(crate) const SESSION_DETAIL_MAX_LIMIT: usize = 500;
