use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::web_repo::WebPersist as _;
use uuid::Uuid;

use crate::store::persistence::{PersistError, PersistHandle};

pub(crate) const PAGE_URL: &str = "http://localhost:9990/wright";

pub(crate) fn line(message_ids: &[i64]) -> String {
    let ids = message_ids
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let first = message_ids.first().copied().unwrap_or_default();
    format!(
        "[Wright] Wright needs a decision from you (message {ids}): open {PAGE_URL}#message-{first}"
    )
}

pub(crate) async fn send_due(persist: &PersistHandle) -> Result<Option<Uuid>, PersistError> {
    let web = persist.web();
    let Some(recipient) = web.wright_operator_session().await? else {
        return Ok(None);
    };
    let due = web.claim_wright_notices().await?;
    if due.is_empty() {
        return Ok(None);
    }
    let sent = persist
        .launch()
        .insert_message(Uuid::new_v4(), recipient, None, &line(&due))
        .await?;
    match sent {
        Some(_) => Ok(Some(recipient)),
        None => {
            tracing::warn!(%recipient, "wright: the operator's session vanished before the notice was queued");
            Ok(None)
        }
    }
}
