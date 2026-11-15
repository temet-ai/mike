use axum::Json;
use axum::extract::State;
use serde_json::{Value, json};

use crate::launch::herdr::prompt::blocked_jev::{POINT, named_key};
use crate::launch::herdr::prompt::blocked_remote::{BlockedDialogAnswer, BlockedDialogRequest};
use crate::state::AppState;

fn answer(dialog: Option<&str>, applies: bool) -> BlockedDialogAnswer {
    BlockedDialogAnswer {
        dialog: dialog.map(str::to_owned),
        layer: if applies { "on" } else { "off" }.to_owned(),
    }
}

fn context_of(request: &BlockedDialogRequest) -> Value {
    match request.session.as_deref() {
        Some(session) => json!({"session": session}),
        None => json!({}),
    }
}

pub async fn post_blocked_dialog(
    State(state): State<AppState>,
    Json(request): Json<BlockedDialogRequest>,
) -> Json<BlockedDialogAnswer> {
    let Some(decider) = state.decider(POINT) else {
        return Json(answer(None, false));
    };
    let applies = decider.applies();
    let dialog = named_key(&decider, &request.screen, context_of(&request)).await;
    Json(answer(dialog, applies))
}

#[cfg(test)]
#[path = "decisions_blocked_dialog_tests.rs"]
mod tests;
