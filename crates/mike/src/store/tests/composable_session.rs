use super::*;

#[tokio::test]
async fn concurrent_claims_yield_exactly_one_first_composable_slot() {
    let store = std::sync::Arc::new(tokio::sync::RwLock::new(RequestStore::new(small_policy(
        200,
    ))));
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..64 {
        let store = std::sync::Arc::clone(&store);
        tasks.spawn(async move {
            let maybe_first = {
                let guard = store.read().await;
                guard.is_first_composable_request_in_session("burst-session")
            };
            if maybe_first {
                let mut guard = store.write().await;
                guard.claim_first_composable_slot("burst-session")
            } else {
                false
            }
        });
    }

    let mut winners = 0;
    while let Some(result) = tasks.join_next().await {
        if result.unwrap_or(false) {
            winners += 1;
        }
    }
    assert_eq!(winners, 1, "exactly one request may open a session");
    assert!(
        !store
            .read()
            .await
            .is_first_composable_request_in_session("burst-session")
    );
}

#[test]
fn claim_first_composable_slot_is_single_use() {
    let mut store = RequestStore::new(small_policy(10));
    assert!(store.claim_first_composable_slot("s1"));
    assert!(!store.claim_first_composable_slot("s1"));
    assert!(store.claim_first_composable_slot("s2"));
}

#[test]
fn is_first_composable_true_for_unknown_session() {
    let store = RequestStore::new(small_policy(10));
    assert!(store.is_first_composable_request_in_session("never-seen"));
}

#[test]
fn is_first_composable_true_after_only_title_sidecar() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));
    let now = Utc::now();
    let sidecar = make_title_sidecar_request(900, "sess-haiku-then-opus", "{}", now);
    store.insert(sidecar);

    assert!(
        store.get_session("sess-haiku-then-opus").is_some(),
        "primary index includes the haiku — that is intentional",
    );
    assert!(
        store.is_first_composable_request_in_session("sess-haiku-then-opus"),
        "composable index must skip title sidecars",
    );
    Ok(())
}

#[test]
fn is_first_composable_false_after_primary_first_request() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));
    let mut req = make_test_request(Some(RequestId::from_raw(910)), "sess-primary-first");
    req.turn_origin = "primary_first".to_string();
    store.insert(req);

    assert!(
        !store.is_first_composable_request_in_session("sess-primary-first"),
        "primary_first insert must close the composable gate"
    );
    Ok(())
}

#[test]
fn is_first_composable_still_true_after_plain_primary_request() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));
    let req = make_test_request(Some(RequestId::from_raw(911)), "sess-plain-primary");
    assert_eq!(req.turn_origin, "primary");
    store.insert(req);

    assert!(
        store.is_first_composable_request_in_session("sess-plain-primary"),
        "plain primary (non-first) must NOT consume the composable slot"
    );
    Ok(())
}

#[test]
fn seed_composable_session_closes_gate_like_a_live_primary_first_insert() {
    let mut store = RequestStore::new(small_policy(10));
    assert!(
        store.is_first_composable_request_in_session("sess-warmup-seeded"),
        "unseeded session starts open"
    );

    store.seed_composable_session("sess-warmup-seeded");

    assert!(
        !store.is_first_composable_request_in_session("sess-warmup-seeded"),
        "warmup seeding must close the gate so a resumed session does not re-compose"
    );
}

#[test]
fn seed_composable_session_is_idempotent() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));
    let mut req = make_test_request(Some(RequestId::from_raw(912)), "sess-already-closed");
    req.turn_origin = "primary_first".to_string();
    store.insert(req);
    assert!(!store.is_first_composable_request_in_session("sess-already-closed"));

    store.seed_composable_session("sess-already-closed");

    assert!(
        !store.is_first_composable_request_in_session("sess-already-closed"),
        "seeding an already-closed session must stay closed"
    );
    Ok(())
}

#[test]
fn is_first_composable_lifecycle_matches_haiku_then_opus_then_followup() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));
    let session_id = "sess-real-claude-p";
    let now = Utc::now();

    let haiku = make_title_sidecar_request(920, session_id, "{}", now);
    store.insert(haiku);
    assert!(
        store.is_first_composable_request_in_session(session_id),
        "haiku must not consume the composable slot",
    );

    let mut opus = make_test_request_with_timestamp(
        Some(RequestId::from_raw(921)),
        session_id,
        now + ChronoDuration::milliseconds(50),
    );
    opus.turn_origin = "primary_first".to_string();
    store.insert(opus);

    assert!(
        !store.is_first_composable_request_in_session(session_id),
        "primary_first insert must close the composable gate",
    );

    let followup = make_test_request_with_timestamp(
        Some(RequestId::from_raw(922)),
        session_id,
        now + ChronoDuration::milliseconds(100),
    );
    store.insert(followup);
    assert!(
        !store.is_first_composable_request_in_session(session_id),
        "follow-up requests must keep the composable gate closed",
    );

    Ok(())
}

#[test]
fn composable_index_survives_session_bucket_eviction() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(2));
    let now = Utc::now();
    let old = now - ChronoDuration::hours(2);

    let mut req_x = make_test_request_with_timestamp(None, "sess-x", old);
    req_x.turn_origin = "primary_first".to_string();
    store.insert(req_x);
    assert!(!store.is_first_composable_request_in_session("sess-x"));

    let req_y = make_test_request_with_timestamp(None, "sess-y", old);
    store.insert(req_y);

    let req_z = make_test_request_with_timestamp(None, "sess-z", now);
    store.insert(req_z);

    assert!(
        store.get_session("sess-x").is_none(),
        "session_index bucket must be gone after eviction",
    );
    assert!(
        !store.is_first_composable_request_in_session("sess-x"),
        "composable index must survive bucket eviction — future requests \
         in the same session must not be re-classified as primary_first",
    );
    Ok(())
}
