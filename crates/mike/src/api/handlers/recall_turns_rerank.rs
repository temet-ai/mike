use crate::api::rerank_search::{RerankReport, arrange, candidates_wanted, score};
use crate::config::AppState;
use crate::store::persistence::RankedTurnRow;

const MAX_FETCH_ROWS: i64 = 200;

pub(super) fn rows_to_fetch(state: &AppState, limit: i64) -> (i64, Option<usize>) {
    match candidates_wanted(state) {
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
        return (arrange(rows, &[]), RerankReport::switched_off());
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
