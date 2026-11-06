use crate::db_helpers;

use super::support::{
    drive_user_prompt, list_turns_for_session, start_upstream_stub, wait_for_request_rows,
};

#[tokio::test]
async fn operator_turn_lifecycle_open_to_closed_atomic() -> anyhow::Result<()> {
    let upstream = start_upstream_stub().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping operator_turn_lifecycle");
        return Ok(());
    };
    let (proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "550e8400-e29b-41d4-a716-446655440081";

    drive_user_prompt(&proxy_url, session_id, "first prompt").await?;
    let after_r1 = wait_for_request_rows(&pool, session_id, 1).await?;
    let (r1_id, r1_created_at, r1_type, r1_origin) = &after_r1[0];
    assert_eq!(r1_type, "user_prompt", "R1 must classify as user_prompt");
    assert_eq!(
        r1_origin, "primary_first",
        "R1 (first in session) must classify as primary_first origin"
    );

    let turns_after_r1 = list_turns_for_session(&pool, session_id).await?;
    assert_eq!(
        turns_after_r1.len(),
        1,
        "exactly one operator_turns row after R1, got {turns_after_r1:?}"
    );
    let t1 = &turns_after_r1[0];
    assert_eq!(t1.session_id, session_id);
    assert_eq!(t1.start_request_id, *r1_id);
    assert_eq!(
        t1.end_request_id, *r1_id,
        "fresh turn's end_request_id must equal start (the INSERT seeds end = start per §4 step 5)"
    );
    assert!(
        t1.closed_at.is_none(),
        "fresh turn must be OPEN (closed_at IS NULL); got {:?}",
        t1.closed_at
    );
    assert_eq!(
        t1.outcome, "unknown",
        "freshly opened turns carry the schema-default outcome until the next user-prompt closes them"
    );
    assert!(
        t1.outcome_evidence_request_id.is_none(),
        "fresh turn must have no evidence FK yet"
    );

    drive_user_prompt(&proxy_url, session_id, "second prompt").await?;
    let after_r2 = wait_for_request_rows(&pool, session_id, 2).await?;
    let (r2_id, r2_created_at, _, _) = &after_r2[1];
    assert_ne!(*r2_id, *r1_id, "R2 must have a fresh request id");
    assert!(
        r2_created_at >= r1_created_at,
        "R2 must be settled at-or-after R1"
    );

    let turns_after_r2 = list_turns_for_session(&pool, session_id).await?;
    assert_eq!(
        turns_after_r2.len(),
        2,
        "exactly two operator_turns rows after R2 (T1 closed + T2 open); got {turns_after_r2:?}"
    );
    let t1_closed = &turns_after_r2[0];
    let t2 = &turns_after_r2[1];

    assert_eq!(t1_closed.turn_id, t1.turn_id, "T1 row must persist");
    assert_eq!(t1_closed.start_request_id, *r1_id);
    assert!(
        t1_closed.closed_at.is_some(),
        "T1 must be CLOSED after R2 settles"
    );
    assert!(
        t1_closed.classified_at.is_some(),
        "T1.classified_at must be stamped by the close UPDATE"
    );
    assert_eq!(
        t1_closed.outcome_evidence_request_id,
        Some(*r2_id),
        "T1.outcome_evidence_request_id must point at R2 (the request that triggered the close)"
    );
    assert_eq!(
        t1_closed.outcome, "unknown",
        "the close UPDATE writes outcome='unknown' — the classifier runs separately"
    );

    assert_eq!(t2.start_request_id, *r2_id);
    assert_eq!(t2.end_request_id, *r2_id);
    assert!(
        t2.closed_at.is_none(),
        "T2 must be OPEN (closed_at IS NULL) until the next user-prompt closes it"
    );
    assert!(
        t2.outcome_evidence_request_id.is_none(),
        "T2 must have no evidence FK — that gets stamped when T2 itself closes"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
