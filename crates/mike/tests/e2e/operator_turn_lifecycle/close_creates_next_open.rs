use crate::db_helpers;

use super::support::{
    drive_user_prompt, list_turns_for_session, start_upstream_stub, wait_for_request_rows,
};

#[tokio::test]
async fn operator_turn_close_creates_next_open_in_same_tx() -> anyhow::Result<()> {
    let upstream = start_upstream_stub().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping close_creates_next_open");
        return Ok(());
    };
    let (proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "550e8400-e29b-41d4-a716-446655440082";

    drive_user_prompt(&proxy_url, session_id, "alpha").await?;
    drive_user_prompt(&proxy_url, session_id, "beta").await?;
    drive_user_prompt(&proxy_url, session_id, "gamma").await?;

    let reqs = wait_for_request_rows(&pool, session_id, 3).await?;
    let r1 = reqs[0].0;
    let r2 = reqs[1].0;
    let r3 = reqs[2].0;

    let turns = list_turns_for_session(&pool, session_id).await?;
    assert_eq!(
        turns.len(),
        3,
        "three operator_turns rows after three user prompts; got {turns:?}"
    );

    assert_eq!(turns[0].start_request_id, r1);
    assert!(
        turns[0].closed_at.is_some(),
        "T1 must be CLOSED after R2's settle wrote closed_at"
    );
    assert_eq!(
        turns[0].outcome_evidence_request_id,
        Some(r2),
        "T1's evidence FK must point at R2 — the request that triggered T1's close"
    );

    assert_eq!(turns[1].start_request_id, r2);
    assert!(
        turns[1].closed_at.is_some(),
        "T2 must be CLOSED after R3's settle wrote closed_at"
    );
    assert_eq!(
        turns[1].outcome_evidence_request_id,
        Some(r3),
        "T2's evidence FK must point at R3"
    );

    assert_eq!(turns[2].start_request_id, r3);
    assert!(
        turns[2].closed_at.is_none(),
        "T3 must be OPEN; got closed_at = {:?}",
        turns[2].closed_at
    );

    let open_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory.operator_turns ot
          JOIN sessions.sessions s ON s.id = ot.session_id
          WHERE s.session_uuid = $1::uuid
            AND ot.closed_at IS NULL",
    )
    .bind(session_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        open_count, 1,
        "the partial UNIQUE index `operator_turns_one_open_per_session` enforces one-open-per-session; \
         after three sequential user prompts we must observe exactly one OPEN turn (the latest)"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
