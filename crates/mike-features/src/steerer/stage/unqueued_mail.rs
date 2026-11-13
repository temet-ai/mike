use crate::state::AppState;
use crate::store::persistence::launch_repo::LaunchPersist as _;

const DRAIN_RELOAD_LIMIT: i64 = 256;

pub async fn queue_mail_from_the_record(state: &AppState, session_id: &str, mailbox_served: bool) {
    if mailbox_served {
        return;
    }
    let Some(persist) = state.persist() else {
        return;
    };
    let Ok(session) = uuid::Uuid::parse_str(session_id) else {
        return;
    };
    if crate::mcp_channel::listeners().is_live(session) {
        return;
    }
    let rows = match persist
        .launch()
        .fetch_mail_for_drain(session, DRAIN_RELOAD_LIMIT)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(session_id = %session, error = %e, "mail waiting in the database could not be read for this turn");
            return;
        }
    };
    if rows.is_empty() {
        return;
    }
    let mut store = state.store.write().await;
    for row in rows {
        store.remove_mailbox_reminder(session_id, row.message_uuid);
        crate::injection::enqueue_if_admitted(
            &state.injection_gate,
            &mut store,
            session_id,
            row.to_reminder(),
        );
    }
}
