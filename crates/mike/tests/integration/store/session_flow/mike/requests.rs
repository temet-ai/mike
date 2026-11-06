use mike::store::persistence::session_flow::{
    FlowActorKind, FlowComponent, FlowOutcome, FlowRetention, FlowRowId, FlowTimeSemantics,
};

use super::super::support::{Fixture, ids, no_raw};
use super::lesson;

#[tokio::test]
async fn session_flow_mike_manifest_outcomes_missing_and_malformed_are_explicit()
-> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let request = f.request(None, "primary").await?;
    let stages: Vec<_> = ["applied", "unchanged", "skipped", "failed_closed"].iter().enumerate().map(|(i, outcome)|
        serde_json::json!({"ordinal":i+1,"stage":format!("stage.{i}"),"outcome":outcome,"resolved_parameters":{"RAW_SENTINEL":true},"targets":[{"RAW_SENTINEL":true}]})).collect();
    sqlx::query("UPDATE requests.requests SET transformation_manifest = $2 WHERE id = $1")
        .bind(request)
        .bind(serde_json::json!({"schema_version":1,"stages":stages}))
        .execute(f.handle.pool())
        .await?;
    let missing = f.request(None, "primary").await?;
    let malformed = f.request(None, "primary").await?;
    sqlx::query("UPDATE requests.requests SET transformation_manifest = '{\"schema_version\":2,\"stages\":\"RAW_SENTINEL\"}' WHERE id = $1")
        .bind(malformed).execute(f.handle.pool()).await?;
    let oversized = f.request(None, "primary").await?;
    sqlx::query("UPDATE requests.requests SET transformation_manifest =
        jsonb_build_object('schema_version',1,'stages',(SELECT jsonb_agg(jsonb_build_object('ordinal',g,'stage','test.stage','outcome','applied')) FROM generate_series(1,10000) g)) WHERE id = $1")
        .bind(oversized).execute(f.handle.pool()).await?;
    let first = f.page(FlowComponent::ManifestStages, 1, None).await?;
    assert_eq!(first.activities.len(), 4);
    assert_eq!(
        first
            .activities
            .iter()
            .map(|a| a.metadata.ordinal)
            .collect::<Vec<_>>(),
        vec![Some(1), Some(2), Some(3), Some(4)]
    );
    assert_eq!(
        first
            .activities
            .iter()
            .map(|a| a.outcome.clone())
            .collect::<Vec<_>>(),
        vec![
            Some(FlowOutcome::Applied),
            Some(FlowOutcome::Unchanged),
            Some(FlowOutcome::Skipped),
            Some(FlowOutcome::FailedClosed)
        ]
    );
    assert!(first.activities.iter().all(
        |a| a.ended_at.is_none() && a.time_semantics == FlowTimeSemantics::RecordedObservation
    ));
    assert_eq!(first.coverage.retention, FlowRetention::Retained);
    let second = f
        .page(
            FlowComponent::ManifestStages,
            10,
            first.next_cursor.as_ref(),
        )
        .await?;
    assert_eq!(second.coverage.lower_id, Some(FlowRowId::Bigint(missing)));
    assert!(second.activities.is_empty() && second.coverage.stale_reason.is_some());
    assert_eq!(second.coverage.retention, FlowRetention::Partial);
    no_raw(&first);
    no_raw(&second);
    f.cleanup().await
}

#[tokio::test]
async fn session_flow_mike_injections_keep_individual_ids_and_nullable_request_attribution()
-> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let (lesson, _) = lesson(&f).await?;
    let mut expected = vec![];
    for session in [Some(f.id), None] {
        let request = f.request(None, "primary").await?;
        expected.push(
            sqlx::query_scalar::<_, i64>(
                "INSERT INTO memory.lesson_injections
            (lesson_id, request_id, session_id, lane_id, persona, injected_at)
            SELECT $1, $2, $3, id, 'same-persona', '2026-09-10T12:00:03Z'
            FROM memory.lesson_injection_lanes WHERE code = 'subagent' RETURNING id",
            )
            .bind(lesson)
            .bind(request)
            .bind(session)
            .fetch_one(f.handle.pool())
            .await?,
        );
    }
    let first = f.page(FlowComponent::LessonInjections, 1, None).await?;
    let second = f
        .page(
            FlowComponent::LessonInjections,
            1,
            first.next_cursor.as_ref(),
        )
        .await?;
    assert_eq!([ids(&first), ids(&second)].concat(), expected);
    assert_eq!(second.activities[0].metadata.lesson_id, Some(lesson));
    assert_eq!(
        second.activities[0].metadata.persona.as_deref(),
        Some("same-persona")
    );
    assert!(
        second
            .actors
            .iter()
            .all(|a| a.kind == FlowActorKind::Mike && a.parent_actor_id.is_none())
    );
    sqlx::query("DELETE FROM memory.lessons WHERE id = $1")
        .bind(lesson)
        .execute(f.handle.pool())
        .await?;
    assert!(
        f.page(FlowComponent::LessonInjections, 10, None)
            .await?
            .activities
            .is_empty()
    );
    no_raw(&first);
    no_raw(&second);
    f.cleanup().await
}
