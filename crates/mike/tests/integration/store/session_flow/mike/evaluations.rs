use mike::store::persistence::session_flow::{FlowComponent, FlowLifecycle, FlowOutcome};

use super::super::support::{Fixture, ids, no_raw};
use super::{intervention, turn};

#[tokio::test]
async fn session_flow_mike_all_evaluations_preserve_verdict_free_pass_and_unknown_end()
-> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let turn = turn(&f).await?;
    let (intervention_id, intervention_uuid) = intervention(&f).await?;
    let mut expected = vec![];
    for (status, verdict) in [
        ("resolved_pass", false),
        ("resolved_pass", true),
        ("resolved_correction", true),
        ("dispatched", false),
    ] {
        expected.push(sqlx::query_scalar::<_, i64>("INSERT INTO steering.evaluations
            (steerer_id, session_id, operator_turn_id, status, intervention_uuid, dispatched_at, resolved_at, verdict_json)
            SELECT id, $1, $2, $3, CASE WHEN $3 = 'resolved_correction' THEN $4::uuid END,
                   '2026-09-10T12:00:00Z', CASE WHEN $3 <> 'dispatched' THEN '2026-09-10T12:00:05Z'::timestamptz END,
                   CASE WHEN $5 THEN '{\"RAW_SENTINEL\":true}'::jsonb END
            FROM steering.steerers WHERE kind = 'verifier' RETURNING id")
            .bind(f.id).bind(turn).bind(status).bind(intervention_uuid).bind(verdict).fetch_one(f.handle.pool()).await?);
    }
    let first = f.page(FlowComponent::VerifierEvaluations, 1, None).await?;
    assert_eq!(first.activities.len(), 2);
    assert!(
        first.activities[0].ended_at.is_none()
            && first.activities[0].metadata.has_verdict.is_none()
    );
    assert_eq!(first.activities[1].metadata.has_verdict, Some(false));
    let rest = f
        .page(
            FlowComponent::VerifierEvaluations,
            10,
            first.next_cursor.as_ref(),
        )
        .await?;
    assert_eq!([ids(&first), ids(&rest)].concat(), expected);
    assert_eq!(
        rest.activities.last().map(|a| a.phase.as_str()),
        Some("dispatched")
    );
    assert!(
        rest.activities
            .last()
            .is_some_and(|a| a.outcome.is_none() && a.ended_at.is_none())
    );
    sqlx::query("DELETE FROM steering.interventions WHERE id = $1")
        .bind(intervention_id)
        .execute(f.handle.pool())
        .await?;
    let refreshed = f.page(FlowComponent::VerifierEvaluations, 10, None).await?;
    assert!(
        refreshed
            .activities
            .iter()
            .all(|a| a.metadata.intervention_uuid.is_none())
    );
    assert_eq!(ids(&refreshed), expected);
    no_raw(&first);
    no_raw(&rest);
    sqlx::query("DELETE FROM memory.operator_turns WHERE id = $1")
        .bind(turn)
        .execute(f.handle.pool())
        .await?;
    assert!(
        f.page(FlowComponent::VerifierEvaluations, 10, None)
            .await?
            .activities
            .is_empty()
    );
    f.cleanup().await
}

#[tokio::test]
async fn session_flow_mike_intervention_attempt_delivery_and_deadline_are_distinct()
-> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let (id, _) = intervention(&f).await?;
    let fired = f.page(FlowComponent::Interventions, 10, None).await?;
    assert_eq!(fired.activities.len(), 1);
    assert_eq!(fired.activities[0].phase, "fired");
    assert!(fired.activities[0].metadata.expires_at.is_some());
    assert!(fired.activities[0].ended_at.is_none());
    assert!(
        fired
            .activities
            .iter()
            .all(|a| a.lifecycle != FlowLifecycle::Expired)
    );
    let request = f.request(None, "primary").await?;
    sqlx::query("UPDATE steering.interventions SET sent_request_id = $2, delivered_request_id = $2,
        sent_at = '2026-09-10T12:00:01Z', processed_at = '2026-09-10T12:00:02Z', reverted_at = '2026-09-10T12:00:03Z'
        WHERE id = $1").bind(id).bind(request).execute(f.handle.pool()).await?;
    let delivered = f.page(FlowComponent::Interventions, 10, None).await?;
    assert_eq!(
        delivered
            .activities
            .iter()
            .map(|a| a.phase.as_str())
            .collect::<Vec<_>>(),
        vec!["fired", "attempted", "delivered", "reverted"]
    );
    assert_eq!(delivered.activities[1].lifecycle, FlowLifecycle::Attempted);
    assert_eq!(delivered.activities[2].lifecycle, FlowLifecycle::Delivered);
    sqlx::query("DELETE FROM requests.requests WHERE id = $1")
        .bind(request)
        .execute(f.handle.pool())
        .await?;
    let retained = f.page(FlowComponent::Interventions, 10, None).await?;
    assert!(
        retained
            .activities
            .iter()
            .any(|a| a.phase == "delivered" && a.source.request_id.is_none())
    );
    no_raw(&fired);
    no_raw(&delivered);
    f.cleanup().await
}

#[tokio::test]
async fn intervention_past_its_deadline_with_no_attempt_is_expired() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let (id, _) = intervention(&f).await?;
    sqlx::query(
        "UPDATE steering.interventions SET expires_at = '2020-01-01T00:00:00Z' WHERE id = $1",
    )
    .bind(id)
    .execute(f.handle.pool())
    .await?;
    let page = f.page(FlowComponent::Interventions, 10, None).await?;
    assert_eq!(page.activities.len(), 1);
    assert_eq!(page.activities[0].phase, "fired");
    assert_eq!(page.activities[0].outcome, Some(FlowOutcome::Expired));
    f.cleanup().await
}
