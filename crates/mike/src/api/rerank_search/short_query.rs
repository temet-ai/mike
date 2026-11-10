use super::RerankReport;
use crate::config::AppState;

pub(crate) fn short_query_report(state: &AppState, query: &str) -> Option<RerankReport> {
    let wanted = state.rerank_service().current_config().min_query_words;
    too_short(query, wanted)
}

pub(super) fn too_short(query: &str, wanted: usize) -> Option<RerankReport> {
    let words = query.split_whitespace().count();
    (words < wanted).then(|| {
        RerankReport::off(&format!(
            "the query has fewer than {wanted} words, and short keyword queries keep plain order"
        ))
    })
}
