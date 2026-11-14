use crate::launch::fleet::decisions::wake::{Mail, hold};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet_repo::FleetPersist as _;

use super::SendContext;

pub(super) async fn hold_directed(
    handle: &PersistHandle,
    state: &AppState,
    ctx: &SendContext<'_>,
    in_reply_to_id: Option<i64>,
) -> bool {
    let Some(from) = ctx.from_session_uuid else {
        return false;
    };
    let sender_label = handle.fleet().sender_label(from).await;
    hold(
        &state.base,
        &Mail {
            to: ctx.to_session_uuid,
            from: Some(from),
            in_reply_to_id,
            body: &ctx.body.message,
            sender_label: sender_label.as_deref(),
            message_uuid: Some(ctx.message_uuid),
        },
    )
    .await
}
