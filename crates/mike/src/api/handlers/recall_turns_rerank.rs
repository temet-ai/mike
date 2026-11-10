use crate::api::rerank_search::{
    RerankReport, arrange, candidates_wanted, score, short_query_report,
};
use crate::api::result_order::ResultOrder;
use crate::config::AppState;
use crate::store::persistence::RankedTurnRow;

const MAX_FETCH_ROWS: i64 = 200;

pub(super) async fn order_turns(
    state: &AppState,
    query: &str,
    rows: Vec<RankedTurnRow>,
    (candidates, order): (Option<usize>, ResultOrder),
) -> (Vec<(RankedTurnRow, Option<f32>)>, RerankReport) {
    if order.is_recent() {
        newest_first(rows)
    } else {
        rerank_turns(state, query, rows, candidates).await
    }
}

fn newest_first(mut rows: Vec<RankedTurnRow>) -> (Vec<(RankedTurnRow, Option<f32>)>, RerankReport) {
    rows.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| format!("turn:{}", a.turn_id).cmp(&format!("turn:{}", b.turn_id)))
    });
    (arrange(rows, &[]), RerankReport::recent_order())
}

pub(super) fn rows_to_fetch(
    state: &AppState,
    limit: i64,
    query: &str,
    recent: bool,
) -> (i64, Option<usize>) {
    if recent {
        let pool = i64::try_from(state.rerank_service().current_config().candidates)
            .unwrap_or(MAX_FETCH_ROWS);
        return (pool.max(limit).min(MAX_FETCH_ROWS), None);
    }
    match candidates_wanted(state).filter(|_| short_query_report(state, query).is_none()) {
        Some(candidates) => {
            let wanted = i64::try_from(candidates).unwrap_or(MAX_FETCH_ROWS);
            (wanted.max(limit), Some(candidates))
        }
        None => (limit, None),
    }
}

pub(super) async fn rerank_turns(
    state: &AppState,
    query: &str,
    rows: Vec<RankedTurnRow>,
    candidates: Option<usize>,
) -> (Vec<(RankedTurnRow, Option<f32>)>, RerankReport) {
    let Some(candidates) = candidates else {
        let report = match candidates_wanted(state) {
            Some(_) => short_query_report(state, query).unwrap_or_else(RerankReport::switched_off),
            None => RerankReport::switched_off(),
        };
        return (arrange(rows, &[]), report);
    };
    let window = rows.len().min(candidates);
    if window == 0 {
        let report = RerankReport::off("there were no results to rerank");
        return (arrange(rows, &[]), report);
    }
    let documents: Vec<String> = rows[..window].iter().map(document).collect();
    match score(state, query, &documents).await {
        Ok(scores) => (arrange(rows, &scores), RerankReport::reranked()),
        Err(report) => (arrange(rows, &[]), report),
    }
}

const PROMPT_HEAD_CHARS: usize = 200;

fn document(row: &RankedTurnRow) -> String {
    turn_text(&row.user_prompt, &row.final_assistant_text)
}

fn turn_text(prompt: &str, reply: &str) -> String {
    let prompt = head(prompt, PROMPT_HEAD_CHARS);
    let reply_room =
        crate::analysis::rerank::MAX_DOCUMENT_CHARS.saturating_sub(prompt.chars().count() + 2);
    format!("{prompt}\n\n{}", head(reply, reply_room))
}

#[cfg(test)]
#[path = "recall_turns_rerank_text_tests.rs"]
mod text_tests;

fn head(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}
