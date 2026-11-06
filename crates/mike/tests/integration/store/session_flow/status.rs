use mike::store::persistence::session_flow::{FlowComponent, FlowLifecycle, FlowTimeSemantics};

use super::support::{Fixture, ids, no_raw};

#[tokio::test]
async fn status_ties_across_channels_use_transition_ids_and_observation_time() -> anyhow::Result<()>
{
    let f = Fixture::new().await?;
    let mut expected = vec![];
    for channel in ["channel-a", "channel-b"] {
        let channel_id: i64 = sqlx::query_scalar(
            "INSERT INTO remote_control.channels (cse_id, local_session_uuid)
            VALUES ($1, $2) RETURNING id",
        )
        .bind(channel)
        .bind(f.session)
        .fetch_one(f.handle.pool())
        .await?;
        for status in ["idle", "requires_action"] {
            expected.push(sqlx::query_scalar::<_, i64>("INSERT INTO remote_control.worker_status_transitions
                (channel_id, to_status, observed_at) VALUES ($1, $2, '2026-09-10T12:00:04Z') RETURNING id")
                .bind(channel_id).bind(status).fetch_one(f.handle.pool()).await?);
        }
    }
    let first = f.page(FlowComponent::WorkerStatus, 2, None).await?;
    let second = f
        .page(FlowComponent::WorkerStatus, 2, first.next_cursor.as_ref())
        .await?;
    assert_eq!([ids(&first), ids(&second)].concat(), expected);
    assert!(
        first
            .activities
            .iter()
            .all(|a| a.occurred_at == Some(a.recorded_at) && a.ended_at.is_none())
    );
    assert_ne!(first.actors[0].id, second.actors[0].id);
    no_raw(&first);
    no_raw(&second);
    f.cleanup().await
}

#[tokio::test]
async fn oversized_status_yields_unknown_lifecycle_and_time_semantics() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let channel_id: i64 = sqlx::query_scalar(
        "INSERT INTO remote_control.channels (cse_id, local_session_uuid)
        VALUES ('channel-malformed', $1) RETURNING id",
    )
    .bind(f.session)
    .fetch_one(f.handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO remote_control.worker_status_transitions (channel_id, to_status, observed_at)
        VALUES ($1, $2, '2026-09-10T12:00:04Z')",
    )
    .bind(channel_id)
    .bind("x".repeat(129))
    .execute(f.handle.pool())
    .await?;
    let page = f.page(FlowComponent::WorkerStatus, 10, None).await?;
    assert_eq!(page.activities.len(), 1);
    assert_eq!(page.activities[0].phase, "status:unknown");
    assert_eq!(page.activities[0].lifecycle, FlowLifecycle::Unknown);
    assert_eq!(
        page.activities[0].time_semantics,
        FlowTimeSemantics::Unknown
    );
    no_raw(&page);
    f.cleanup().await
}
