use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::web_repo::WebPersist as _;
use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::wright::FixBatchRow;
use crate::wright::conversation::notice::PAGE_URL;

pub(super) fn notice(row: &FixBatchRow, state: &str, reason: &str, branch: &str) -> String {
    format!(
        "[Wright] The fixes of {day} (batch {id}) are {state} and were not merged: {reason}. Branch {branch} is held in {worktree}. Open {PAGE_URL} to see the batch.",
        day = row.day,
        id = row.id,
        worktree = row.worktree,
    )
}

pub(super) async fn tell_operator(
    persist: &PersistHandle,
    row: &FixBatchRow,
    state: &str,
    reason: &str,
    branch: &str,
) {
    let text = notice(row, state, reason, branch);
    let Some(recipient) = operator(persist, row.id).await else {
        return;
    };
    let sent = persist
        .launch()
        .insert_message(Uuid::new_v4(), recipient, None, &text)
        .await;
    if let Err(e) = sent {
        log_problem(
            row.id,
            "the operator could not be told about a held batch",
            &e,
        );
    }
}

async fn operator(persist: &PersistHandle, batch: i64) -> Option<Uuid> {
    match persist.web().wright_operator_session().await {
        Ok(Some(recipient)) => Some(recipient),
        Ok(None) => {
            log_problem(batch, "no operator session is live to tell", &"none");
            None
        }
        Err(e) => {
            log_problem(batch, "the operator session could not be looked up", &e);
            None
        }
    }
}

fn log_problem(batch: i64, what: &str, error: &dyn std::fmt::Display) {
    tracing::warn!(batch, %error, "wright repair: a held batch: {what}");
}
