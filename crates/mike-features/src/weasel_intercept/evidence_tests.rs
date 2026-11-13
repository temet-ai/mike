use super::evidence::{
    InterceptEvidence, InterceptOutcome, note_unscanned_buffered, record, weasel_armed,
};
use super::test_fixture::{armed_toggles, drain_spawned_writes, fixture, rows_for};
use crate::store::models::RequestId;
use crate::store::persistence::test_support::{seed_captured_request, seed_project};

#[tokio::test]
async fn record_writes_a_row_keyed_by_session_and_request() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let evidence = InterceptEvidence::new(
        &fx.session_uuid,
        RequestId::from_raw(fx.request_id as u64),
        InterceptOutcome::WeaselCorrection,
        Some("pre-existing".to_owned()),
    );

    assert!(record(&fx.state, &evidence).await);

    let rows = rows_for(&fx).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].request_id, Some(fx.request_id));
    assert_eq!(rows[0].outcome, "weasel_correction");
    assert_eq!(rows[0].subject.as_deref(), Some("pre-existing"));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn record_keeps_the_event_when_the_request_was_never_persisted() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let evidence = InterceptEvidence::new(
        &fx.session_uuid,
        RequestId::fallback(),
        InterceptOutcome::WeaselCorrection,
        Some("pre-existing".to_owned()),
    );

    assert!(record(&fx.state, &evidence).await);

    let rows = rows_for(&fx).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].request_id, None,
        "a fallback id names no row, so the event is kept under the session alone"
    );
    assert_eq!(rows[0].subject.as_deref(), Some("pre-existing"));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn record_for_an_unknown_session_reports_failure_and_writes_nothing() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let stranger = uuid::Uuid::new_v4().to_string();
    let evidence = InterceptEvidence::new(
        &stranger,
        RequestId::from_raw(fx.request_id as u64),
        InterceptOutcome::ProxyToolAnswer,
        Some("mike_session_roster".to_owned()),
    );

    assert!(!record(&fx.state, &evidence).await);
    assert!(rows_for(&fx).await?.is_empty());
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn record_rejects_a_session_id_that_is_not_a_uuid() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let evidence = InterceptEvidence::new(
        "not-a-uuid",
        RequestId::from_raw(fx.request_id as u64),
        InterceptOutcome::WeaselCorrection,
        None,
    );

    assert!(!record(&fx.state, &evidence).await);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn armed_session_with_a_buffered_success_is_marked_not_scanned() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    armed_toggles(&fx.state, &["pre-existing"]);
    let toggles = fx.state.resolve_session_toggles(&fx.session_uuid);
    assert!(weasel_armed(&toggles, &fx.session_uuid));

    note_unscanned_buffered(
        &fx.state,
        &toggles,
        true,
        &fx.session_uuid,
        RequestId::from_raw(fx.request_id as u64),
    );
    drain_spawned_writes(&fx.state).await;

    let rows = rows_for(&fx).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].outcome, "not_scanned_buffered");
    assert_eq!(rows[0].request_id, Some(fx.request_id));
    assert_eq!(rows[0].subject, None);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn buffered_upstream_error_is_not_marked_not_scanned() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    armed_toggles(&fx.state, &["pre-existing"]);
    let toggles = fx.state.resolve_session_toggles(&fx.session_uuid);

    note_unscanned_buffered(
        &fx.state,
        &toggles,
        false,
        &fx.session_uuid,
        RequestId::from_raw(fx.request_id as u64),
    );
    drain_spawned_writes(&fx.state).await;

    assert!(rows_for(&fx).await?.is_empty());
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn disarmed_session_leaves_no_not_scanned_row() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    armed_toggles(&fx.state, &[]);
    let toggles = fx.state.resolve_session_toggles(&fx.session_uuid);
    assert!(!weasel_armed(&toggles, &fx.session_uuid));

    note_unscanned_buffered(
        &fx.state,
        &toggles,
        true,
        &fx.session_uuid,
        RequestId::from_raw(fx.request_id as u64),
    );
    drain_spawned_writes(&fx.state).await;

    assert!(rows_for(&fx).await?.is_empty());
    fx.db.cleanup().await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct InterceptFlagRow {
    intercepted: bool,
    correction_request_id: Option<i64>,
}

async fn flags_of(
    fx: &super::test_fixture::Fixture,
    request_id: i64,
) -> anyhow::Result<InterceptFlagRow> {
    let persist = fx
        .state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persistence must be on"))?;
    let row = sqlx::query_as::<_, InterceptFlagRow>(
        "SELECT intercepted, correction_request_id FROM requests.requests WHERE id = $1",
    )
    .bind(request_id)
    .fetch_one(persist.pool())
    .await?;
    Ok(row)
}

#[tokio::test]
async fn mark_intercepted_flags_the_original_when_the_correction_never_persisted()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let persist = fx
        .state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persistence must be on"))?;

    persist
        .mark_intercepted(fx.request_id, RequestId::fallback().as_u64() as i64)
        .await;

    let flags = flags_of(&fx, fx.request_id).await?;
    assert!(
        flags.intercepted,
        "a fallback correction id must not void the flag on the original"
    );
    assert_eq!(flags.correction_request_id, None);
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn mark_intercepted_links_the_correction_when_both_persisted() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let persist = fx
        .state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persistence must be on"))?;
    let project_id = seed_project(persist.pool(), "weasel-evidence-correction").await?;
    let correction_id = seed_captured_request(
        persist.pool(),
        &uuid::Uuid::new_v4().to_string(),
        project_id,
        "primary",
        b"correction request",
    )
    .await?;

    persist.mark_intercepted(fx.request_id, correction_id).await;

    let flags = flags_of(&fx, fx.request_id).await?;
    assert!(flags.intercepted);
    assert_eq!(flags.correction_request_id, Some(correction_id));
    fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn mark_intercepted_leaves_other_rows_alone_when_the_original_never_persisted()
-> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };
    let persist = fx
        .state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persistence must be on"))?;

    persist
        .mark_intercepted(RequestId::fallback().as_u64() as i64, fx.request_id)
        .await;

    let flags = flags_of(&fx, fx.request_id).await?;
    assert!(!flags.intercepted);
    assert_eq!(flags.correction_request_id, None);
    fx.db.cleanup().await?;
    Ok(())
}
