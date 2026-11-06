use uuid::Uuid;

use mike::store::persistence::session_flow::{
    FlowActorKind, FlowComponent, FlowLifecycle, FlowRelationshipKind, FlowRowId,
};

use super::support::{Fixture, ids, no_raw};

#[tokio::test]
async fn self_anonymous_open_peer_and_reply_messages_keep_distinct_phases() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let peer = Uuid::new_v4();
    let peer_id = mike::store::persistence::id_resolvers::resolve_session_id(
        f.handle.pool(),
        &peer.to_string(),
        None,
        None,
    )
    .await?;
    let thread: i64 = sqlx::query_scalar("INSERT INTO mailbox.threads (thread_uuid, subject) VALUES ($1, 'RAW_SENTINEL') RETURNING id")
        .bind(Uuid::new_v4()).fetch_one(f.handle.pool()).await?;
    let mut expected = vec![];
    for (sender, recipient) in [
        (Some(f.id), Some(f.id)),
        (None, Some(f.id)),
        (Some(f.id), None),
        (Some(f.id), Some(peer_id)),
    ] {
        let id: i64 = sqlx::query_scalar("INSERT INTO mailbox.messages (message_uuid, from_session_id, to_session_id, body, thread_id, in_reply_to_id)
            VALUES ($1, $2, $3, 'RAW_SENTINEL', $4, $5) RETURNING id")
            .bind(Uuid::new_v4()).bind(sender).bind(recipient).bind(thread).bind(expected.first().copied()).fetch_one(f.handle.pool()).await?;
        expected.push(id);
    }
    let queued = f.page(FlowComponent::Messages, 10, None).await?;
    assert_eq!(ids(&queued), expected);
    assert_eq!(queued.activities.len(), 4);
    assert!(
        queued
            .relationships
            .iter()
            .all(|r| r.kind != FlowRelationshipKind::MessageRecipient)
    );
    assert!(
        queued
            .relationships
            .iter()
            .any(|r| r.kind == FlowRelationshipKind::MessageSender && r.from_id.is_none())
    );
    assert!(
        queued
            .relationships
            .iter()
            .any(|r| r.kind == FlowRelationshipKind::MessageReply)
    );
    assert!(
        queued
            .activities
            .iter()
            .all(|a| a.metadata.thread_id == Some(thread))
    );
    let carrier = f.request(None, "primary").await?;
    sqlx::query("UPDATE mailbox.messages SET claimed_at = now(), sent_at = now(), sent_request_id = $2, consumed_at = now() WHERE id = $1")
        .bind(expected[0]).bind(carrier).execute(f.handle.pool()).await?;
    let later = f.page(FlowComponent::Messages, 10, None).await?;
    let phases: Vec<_> = later
        .activities
        .iter()
        .filter(|a| a.source.row_id == FlowRowId::Bigint(expected[0]))
        .map(|a| (a.phase.as_str(), a.lifecycle.clone()))
        .collect();
    assert_eq!(
        phases,
        vec![
            ("queued", FlowLifecycle::Queued),
            ("claimed", FlowLifecycle::Claimed),
            ("sent", FlowLifecycle::Delivered),
            ("consumed", FlowLifecycle::Consumed)
        ]
    );
    sqlx::query("UPDATE mailbox.messages SET channel_delivered_at = now() WHERE id = $1")
        .bind(expected[1])
        .execute(f.handle.pool())
        .await?;
    let pushed = f.page(FlowComponent::Messages, 10, None).await?;
    let Some(push) = pushed
        .activities
        .iter()
        .find(|a| a.source.row_id == FlowRowId::Bigint(expected[1]) && a.phase == "pushed")
    else {
        anyhow::bail!("channel push emitted no pushed activity");
    };
    assert_eq!(push.lifecycle, FlowLifecycle::Delivered);
    assert!(pushed.relationships.iter().any(|r| {
        r.kind == FlowRelationshipKind::MessageRecipient
            && r.from_id.as_deref() == Some(push.id.as_str())
    }));
    no_raw(&queued);
    no_raw(&later);
    f.cleanup().await
}

#[tokio::test]
async fn peer_actor_bound_keeps_endpoint_ids_and_reports_omission() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let peer_ids: Vec<i64> = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id)
        SELECT gen_random_uuid(), $1 FROM generate_series(1, 129) RETURNING id",
    )
    .bind(f.project)
    .fetch_all(f.handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO mailbox.messages (message_uuid, from_session_id, to_session_id, body, sent_at)
        SELECT gen_random_uuid(), $1, peer, 'RAW_SENTINEL', now() FROM unnest($2::bigint[]) peer",
    )
    .bind(f.id)
    .bind(peer_ids)
    .execute(f.handle.pool())
    .await?;
    let page = f.page(FlowComponent::Messages, 500, None).await?;
    assert_eq!(
        page.actors
            .iter()
            .filter(|a| a.kind == FlowActorKind::Peer)
            .count(),
        128
    );
    assert_eq!(
        page.relationships
            .iter()
            .filter(|r| r.kind == FlowRelationshipKind::MessageRecipient && r.to_id.is_some())
            .count(),
        129
    );
    assert!(
        page.coverage
            .stale_reason
            .as_ref()
            .is_some_and(|v| v.contains("peer_actor_limit"))
    );
    assert!(page.coverage.exhausted);
    no_raw(&page);
    f.cleanup().await
}

#[tokio::test]
async fn deleted_sender_preserves_message_with_unknown_endpoint() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let sender = mike::store::persistence::id_resolvers::resolve_session_id(
        f.handle.pool(),
        &Uuid::new_v4().to_string(),
        None,
        None,
    )
    .await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO mailbox.messages (message_uuid, from_session_id, to_session_id, body)
        VALUES ($1, $2, $3, 'RAW_SENTINEL') RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(sender)
    .bind(f.id)
    .fetch_one(f.handle.pool())
    .await?;
    sqlx::query("DELETE FROM sessions.sessions WHERE id = $1")
        .bind(sender)
        .execute(f.handle.pool())
        .await?;
    let page = f.page(FlowComponent::Messages, 10, None).await?;
    assert_eq!(ids(&page), vec![id]);
    assert!(
        page.relationships
            .iter()
            .any(|r| r.kind == FlowRelationshipKind::MessageSender && r.from_id.is_none())
    );
    f.cleanup().await
}
