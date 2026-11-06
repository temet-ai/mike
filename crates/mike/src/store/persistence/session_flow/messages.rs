use super::*;

struct Row {
    id: i64,
    thread_id: Option<i64>,
    from_session: Option<Uuid>,
    to_session: Option<Uuid>,
    in_reply_to_id: Option<i64>,
    sent_request_id: Option<i64>,
    queued_at: DateTime<Utc>,
    claimed_at: Option<DateTime<Utc>>,
    sent_at: Option<DateTime<Utc>>,
    channel_delivered_at: Option<DateTime<Utc>>,
    consumed_at: Option<DateTime<Utc>>,
    dead_lettered_at: Option<DateTime<Utc>>,
}

pub(super) async fn read(
    tx: &mut sqlx::PgConnection,
    page: &mut FlowPage,
    ids: &[i64],
) -> Result<(), PersistError> {
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT m.id, m.thread_id, f.session_uuid AS "from_session?", t.session_uuid AS "to_session?",
                  m.in_reply_to_id, m.sent_request_id, m.queued_at, m.claimed_at,
                  m.sent_at, m.channel_delivered_at, m.consumed_at, m.dead_lettered_at
             FROM mailbox.messages m LEFT JOIN sessions.sessions f ON f.id = m.from_session_id
             LEFT JOIN sessions.sessions t ON t.id = m.to_session_id
            WHERE m.id = ANY($1) ORDER BY m.id"#,
        ids,
    )
    .fetch_all(tx)
    .await?;
    for row in rows {
        append(page, row);
    }
    Ok(())
}

fn append(page: &mut FlowPage, row: Row) {
    let source = build::source("mailbox.messages", row.id, page.session_id, None);
    let actor = build::actor(
        page.session_id,
        FlowActorKind::ToolService,
        format!("mailbox:{}", page.session_id),
        "Mailbox".into(),
        Some(source.clone()),
    );
    for session in [row.from_session, row.to_session].into_iter().flatten() {
        let peer = session != page.session_id;
        page.actors.push(build::actor(
            session,
            if peer {
                FlowActorKind::Peer
            } else {
                FlowActorKind::Session
            },
            format!("session:{session}"),
            if peer { "Peer" } else { "Session" }.into(),
            None,
        ));
    }
    let phases = [
        ("queued", Some(row.queued_at), FlowLifecycle::Queued),
        ("claimed", row.claimed_at, FlowLifecycle::Claimed),
        ("sent", row.sent_at, FlowLifecycle::Delivered),
        ("pushed", row.channel_delivered_at, FlowLifecycle::Delivered),
        ("consumed", row.consumed_at, FlowLifecycle::Consumed),
        (
            "dead_lettered",
            row.dead_lettered_at,
            FlowLifecycle::Expired,
        ),
    ];
    for (phase, time, lifecycle) in phases {
        let Some(time) = time else {
            continue;
        };
        let mut activity = build::activity(
            source.clone(),
            phase,
            actor.id.clone(),
            FlowComponent::Messages,
            time,
        );
        activity.lifecycle = lifecycle;
        activity.metadata.thread_id = row.thread_id;
        if phase == "sent" {
            activity.source.request_id = row.sent_request_id;
            activity.source.detail_href =
                row.sent_request_id.map(|id| format!("/api/requests/{id}"));
        }
        if phase == "sent" || phase == "pushed" {
            page.relationships.push(build::relationship(
                &activity,
                FlowRelationshipKind::MessageRecipient,
                "recipient",
                Some(activity.id.clone()),
                row.to_session.map(|id| format!("session:{id}")),
            ));
        }
        if phase == "queued" {
            page.relationships.push(build::relationship(
                &activity,
                FlowRelationshipKind::MessageSender,
                "sender",
                row.from_session.map(|id| format!("session:{id}")),
                Some(activity.id.clone()),
            ));
            if let Some(reply) = row.in_reply_to_id {
                page.relationships.push(build::relationship(
                    &activity,
                    FlowRelationshipKind::MessageReply,
                    "reply",
                    Some(format!("mailbox.messages:{reply}:queued")),
                    Some(activity.id.clone()),
                ));
            }
        }
        page.activities.push(activity);
    }
    page.actors.push(actor);
}
