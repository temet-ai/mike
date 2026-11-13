use crate::store::persistence::eval_repo::EvalPersist as _;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use super::runs::{error, unavailable};
use crate::state::AppState;
use crate::store::persistence::eval_runs::{
    EvalReviewContext, EvalReviewLabels, EvalRunItemRow, RunItemsFilter,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TextState {
    Present,
    EmptyTurn,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NextMessageState {
    Present,
    None,
    Pending,
    Missing,
    Unknown,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewContextView {
    #[serde(flatten)]
    context: EvalReviewContext,
    prompt_state: TextState,
    assistant_state: TextState,
    next_message_state: NextMessageState,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewDetail {
    item: EvalRunItemRow,
    context: Option<ReviewContextView>,
    labels: EvalReviewLabels,
}

fn has_text(text: Option<&str>) -> bool {
    text.is_some_and(|t| !t.trim().is_empty())
}

pub(crate) fn prompt_state(prompt: Option<&str>) -> TextState {
    if has_text(prompt) {
        TextState::Present
    } else {
        TextState::Missing
    }
}

pub(crate) fn assistant_state(text: Option<&str>, text_chars: Option<i32>) -> TextState {
    if has_text(text) {
        return TextState::Present;
    }
    match text_chars {
        Some(0) => TextState::EmptyTurn,
        _ => TextState::Missing,
    }
}

pub(crate) fn next_message_state(
    facts_present: bool,
    next_state: Option<&str>,
    text: Option<&str>,
) -> NextMessageState {
    if !facts_present {
        return NextMessageState::Unknown;
    }
    match next_state {
        Some("resolved") if has_text(text) => NextMessageState::Present,
        Some("resolved") => NextMessageState::Missing,
        Some("pending") => NextMessageState::Pending,
        Some("none_session_settled") => NextMessageState::None,
        _ => NextMessageState::Unknown,
    }
}

fn view(context: EvalReviewContext) -> ReviewContextView {
    ReviewContextView {
        prompt_state: prompt_state(context.user_prompt.as_deref()),
        assistant_state: assistant_state(
            context.final_assistant_text.as_deref(),
            context.text_chars,
        ),
        next_message_state: next_message_state(
            context.facts_present,
            context.next_state.as_deref(),
            context.next_genuine_text.as_deref(),
        ),
        context,
    }
}

fn internal(e: impl std::fmt::Display) -> Response {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("persistence error: {e}"),
    )
}

pub(crate) async fn get_eval_run_item_review(
    State(state): State<AppState>,
    Path((run_id, turn_id)): Path<(i64, i64)>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let filter = RunItemsFilter {
        operator_turn_id: Some(turn_id),
        include_verdict: true,
        include_review: true,
        limit: 1,
        ..RunItemsFilter::default()
    };
    let item = match persist.eval().eval_run_items_page(run_id, &filter).await {
        Ok(page) => page.items.into_iter().next(),
        Err(e) => return internal(e),
    };
    let Some(item) = item else {
        return error(
            StatusCode::NOT_FOUND,
            format!("eval run {run_id} has no item for turn {turn_id}"),
        );
    };
    let context = match persist.eval().eval_review_context(turn_id).await {
        Ok(c) => c.map(view),
        Err(e) => return internal(e),
    };
    match persist.eval().eval_review_labels(run_id, turn_id).await {
        Ok(labels) => Json(ReviewDetail {
            item,
            context,
            labels,
        })
        .into_response(),
        Err(e) => internal(e),
    }
}

#[cfg(test)]
#[path = "review_tests.rs"]
mod tests;
