use super::*;

struct Row {
    id: i64,
    channel_id: i64,
    to_status: Option<String>,
    observed_at: DateTime<Utc>,
}

pub(super) async fn read(
    tx: &mut sqlx::PgConnection,
    page: &mut FlowPage,
    ids: &[i64],
) -> Result<(), PersistError> {
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT id, channel_id,
                  CASE WHEN octet_length(to_status) <= 128 THEN to_status END AS "to_status?",
                  observed_at
             FROM remote_control.worker_status_transitions WHERE id = ANY($1) ORDER BY id"#,
        ids,
    )
    .fetch_all(tx)
    .await?;
    for row in rows {
        let source = build::source(
            "remote_control.worker_status_transitions",
            row.id,
            page.session_id,
            None,
        );
        let actor = build::actor(
            page.session_id,
            FlowActorKind::Worker,
            format!("channel:{}:{}", page.session_id, row.channel_id),
            format!("Worker channel {}", row.channel_id),
            Some(source.clone()),
        );
        let status_label = build::vocabulary(row.to_status);
        let phase = format!(
            "status:{}",
            status_label.clone().unwrap_or_else(|| "unknown".into())
        );
        let mut activity = build::activity(
            source,
            &phase,
            actor.id.clone(),
            FlowComponent::WorkerStatus,
            row.observed_at,
        );
        if status_label.is_none() {
            activity.lifecycle = FlowLifecycle::Unknown;
            activity.time_semantics = FlowTimeSemantics::Unknown;
        }
        page.actors.push(actor);
        page.activities.push(activity);
    }
    Ok(())
}
