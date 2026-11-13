use crate::store::persistence::launch_repo::LaunchPersist as _;
pub(super) async fn record_failed_send(
    handle: &crate::store::persistence::PersistHandle,
    attempted_by: Option<uuid::Uuid>,
    attempted_to: uuid::Uuid,
    reason: &'static str,
) {
    if let Err(e) = handle
        .launch()
        .insert_failed_send(attempted_by, &attempted_to.to_string(), reason, None)
        .await
    {
        tracing::warn!(
            error = %e,
            reason,
            %attempted_to,
            "post_session_message: failed_sends audit insert failed"
        );
    }
}
