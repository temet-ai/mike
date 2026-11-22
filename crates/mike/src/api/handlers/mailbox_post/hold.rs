use crate::launch::fleet::decisions::wake::{HoldCandidate, Mail, hold_candidate};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;

use super::SendContext;

pub(super) struct Hold {
    candidate: HoldCandidate,
    sender_label: Option<String>,
    in_reply_to_id: Option<i64>,
}

impl Hold {
    pub(super) fn provisional(hold: Option<&Self>) -> bool {
        hold.is_some_and(|hold| hold.candidate.applies())
    }
}

pub(super) async fn hold_candidate_for(
    handle: &PersistHandle,
    state: &AppState,
    ctx: &SendContext<'_>,
    in_reply_to_id: Option<i64>,
) -> Option<Hold> {
    let from = ctx.from_session_uuid?;
    let sender_label = handle.fleet().sender_label(from).await;
    let candidate = hold_candidate(
        &state.base,
        &mail(ctx, in_reply_to_id, sender_label.as_deref()),
    )
    .await?;
    Some(Hold {
        candidate,
        sender_label,
        in_reply_to_id,
    })
}

pub(super) async fn settle_hold(
    handle: &PersistHandle,
    ctx: &SendContext<'_>,
    hold: Option<Hold>,
) -> bool {
    let Some(hold) = hold else {
        return false;
    };
    let provisional = Hold::provisional(Some(&hold));
    let held = hold
        .candidate
        .decide(
            handle,
            &mail(ctx, hold.in_reply_to_id, hold.sender_label.as_deref()),
        )
        .await;
    if provisional && !held {
        if let Err(e) = handle.launch().release_held_message(ctx.message_uuid).await {
            tracing::warn!(error = %e, message_uuid = %ctx.message_uuid, "post_session_message: releasing a provisionally held message failed");
            return true;
        }
    }
    held
}

fn mail<'a>(
    ctx: &'a SendContext<'_>,
    in_reply_to_id: Option<i64>,
    sender_label: Option<&'a str>,
) -> Mail<'a> {
    Mail {
        to: ctx.to_session_uuid,
        from: ctx.from_session_uuid,
        in_reply_to_id,
        body: &ctx.body.message,
        sender_label,
        message_uuid: Some(ctx.message_uuid),
    }
}
