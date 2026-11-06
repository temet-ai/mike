use crate::db_helpers;

use super::support::{
    drive_body, drive_user_prompt, list_turns_for_session, start_upstream_stub,
    suggestion_probe_body, wait_for_request_rows,
};

#[tokio::test]
async fn operator_turn_extends_on_next_input_suggestion_probe() -> anyhow::Result<()> {
    let upstream = start_upstream_stub().await;
    let Some(proxy) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping suggestion-probe lifecycle test");
        return Ok(());
    };
    let (proxy_url, _inspect_url, shutdown, pool, test_db) = proxy.into_parts();

    let session_id = "550e8400-e29b-41d4-a716-446655440083";
    let prior_prompt = "fix the bug in parser.rs";

    drive_user_prompt(&proxy_url, session_id, prior_prompt).await?;
    let after_r1 = wait_for_request_rows(&pool, session_id, 1).await?;
    let (r1_id, _, _, r1_origin) = &after_r1[0];
    assert_eq!(r1_origin, "primary_first");

    let turns_after_r1 = list_turns_for_session(&pool, session_id).await?;
    assert_eq!(turns_after_r1.len(), 1, "R1 opens exactly one turn");
    let t1 = &turns_after_r1[0];
    assert_eq!(t1.start_request_id, *r1_id);
    assert!(t1.closed_at.is_none(), "T1 must be OPEN after R1");

    drive_body(
        &proxy_url,
        session_id,
        &suggestion_probe_body(prior_prompt, "here is the fix"),
    )
    .await?;
    let after_r2 = wait_for_request_rows(&pool, session_id, 2).await?;
    let (r2_id, _, r2_type, r2_origin) = &after_r2[1];
    assert_ne!(*r2_id, *r1_id, "the probe must be a fresh request row");
    assert_eq!(
        r2_type, "user_prompt",
        "the probe still classifies as a user_prompt turn_type"
    );
    assert_eq!(
        r2_origin, "next_input_suggestion_sidecar",
        "the probe must classify as next_input_suggestion_sidecar, not primary"
    );

    let turns_after_r2 = list_turns_for_session(&pool, session_id).await?;
    assert_eq!(
        turns_after_r2.len(),
        1,
        "the probe must NOT open a new operator turn; got {turns_after_r2:?}"
    );
    let t1_after_probe = &turns_after_r2[0];
    assert_eq!(
        t1_after_probe.turn_id, t1.turn_id,
        "the original turn T1 must persist unchanged in identity"
    );
    assert_eq!(
        t1_after_probe.start_request_id, *r1_id,
        "T1's start must stay anchored at R1 — the probe never becomes a start"
    );
    assert_eq!(
        t1_after_probe.end_request_id, *r2_id,
        "T1's trailing edge must extend to the probe's request id, exactly like any other sidecar riding inside the span"
    );
    assert!(
        t1_after_probe.closed_at.is_none(),
        "T1 must remain OPEN — the probe must not trigger a close (it is not `primary`/`primary_first`)"
    );
    assert!(
        t1_after_probe.outcome_evidence_request_id.is_none(),
        "T1 must have no evidence FK — it hasn't closed"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
