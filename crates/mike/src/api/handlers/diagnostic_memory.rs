use super::*;

pub(crate) const MEMORY_TOP_DEFAULT: u32 = 10;

pub(crate) const MEMORY_TOP_MAX: u32 = 100;

pub(crate) const MEMORY_TOP_MIN: u32 = 1;

#[derive(Serialize)]
pub(crate) struct MemoryBreakdownDto {
    pub(crate) entry_count: u64,
    pub(crate) request_body_bytes: u64,
    pub(crate) response_body_bytes: u64,
    pub(crate) filtered_request_body_bytes: u64,
    pub(crate) tool_calls_bytes: u64,
    pub(crate) tool_results_bytes: u64,
    pub(crate) tool_results_in_body_bytes: u64,
    pub(crate) system_prompt_blocks_bytes: u64,
    pub(crate) failed_stream_blocks_bytes: u64,
    pub(crate) entries_total_bytes: u64,
    pub(crate) session_index_keys: u64,
    pub(crate) composable_session_index_keys: u64,
    pub(crate) session_meta_keys: u64,
    pub(crate) in_flight_count: u64,
    pub(crate) process_rss_bytes: Option<u64>,

    pub(crate) max_entries: u64,
    pub(crate) max_store_bytes: u64,
    pub(crate) over_byte_budget: bool,

    pub(crate) top_entries: Vec<MemoryEntryDto>,
}

impl From<&MemoryBreakdownView> for MemoryBreakdownDto {
    fn from(view: &MemoryBreakdownView) -> Self {
        Self {
            entry_count: view.entry_count,
            request_body_bytes: view.request_body_bytes,
            response_body_bytes: view.response_body_bytes,
            filtered_request_body_bytes: view.filtered_request_body_bytes,
            tool_calls_bytes: view.tool_calls_bytes,
            tool_results_bytes: view.tool_results_bytes,
            tool_results_in_body_bytes: view.tool_results_in_body_bytes,
            system_prompt_blocks_bytes: view.system_prompt_blocks_bytes,
            failed_stream_blocks_bytes: view.failed_stream_blocks_bytes,
            entries_total_bytes: view.entries_total_bytes,
            session_index_keys: view.session_index_keys,
            composable_session_index_keys: view.composable_session_index_keys,
            session_meta_keys: view.session_meta_keys,
            in_flight_count: view.in_flight_count,
            process_rss_bytes: view.process_rss_bytes,
            max_entries: view.max_entries,
            max_store_bytes: view.max_store_bytes,
            over_byte_budget: view.over_byte_budget,
            top_entries: view.top_entries.iter().map(MemoryEntryDto::from).collect(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct MemoryEntryDto {
    pub(crate) id: u64,
    pub(crate) session_id: String,
    pub(crate) status: String,
    pub(crate) created_at: String,
    pub(crate) request_body_bytes: u64,
    pub(crate) response_body_bytes: u64,
    pub(crate) filtered_request_body_bytes: u64,
    pub(crate) total_bytes: u64,
}

impl From<&MemoryEntryRow> for MemoryEntryDto {
    fn from(row: &MemoryEntryRow) -> Self {
        Self {
            id: row.id.as_u64(),
            session_id: row.session_id.clone(),
            status: row.status.clone(),
            created_at: row.created_at.to_rfc3339(),
            request_body_bytes: row.request_body_bytes,
            response_body_bytes: row.response_body_bytes,
            filtered_request_body_bytes: row.filtered_request_body_bytes,
            total_bytes: row.total_bytes,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct MemoryParams {
    pub(crate) top: Option<u32>,
}

pub(crate) async fn get_diagnostic_memory(
    State(state): State<AppState>,
    Query(params): Query<MemoryParams>,
) -> Result<Json<MemoryBreakdownDto>, StatusCode> {
    if state.persist().is_none() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let top = params
        .top
        .unwrap_or(MEMORY_TOP_DEFAULT)
        .clamp(MEMORY_TOP_MIN, MEMORY_TOP_MAX);

    let mut view = {
        let store = state.store.read().await;
        store.memory_breakdown(top as usize)
    };

    view.in_flight_count = state.in_flight_len() as u64;
    view.process_rss_bytes = crate::diagnostics::rss::read_max_rss();

    Ok(Json(MemoryBreakdownDto::from(&view)))
}
