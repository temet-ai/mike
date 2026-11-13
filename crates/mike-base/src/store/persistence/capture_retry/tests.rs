use super::*;

#[test]
fn classify_pool_timeout_is_temporary_and_names_the_timeout() {
    let failure = classify(&PersistError::Database(sqlx::Error::PoolTimedOut));
    assert_eq!(
        failure,
        WriteFailure::Temporary("pool timed out waiting for an open connection".to_string())
    );
}

#[test]
fn classify_code_lock_and_statement_timeouts_are_temporary_and_named() {
    assert!(matches!(
        classify_code(Some("55P03"), "canceling statement due to lock timeout"),
        WriteFailure::Temporary(r) if r.starts_with("lock timeout")
    ));
    assert!(matches!(
        classify_code(Some("57014"), "canceling statement due to statement timeout"),
        WriteFailure::Temporary(r) if r.starts_with("statement timeout")
    ));
}

#[test]
fn classify_code_data_exception_is_refused() {
    assert!(matches!(
        classify_code(Some("22P05"), "unsupported Unicode escape sequence"),
        WriteFailure::Refused(_)
    ));
    assert!(matches!(
        classify_code(
            Some("22021"),
            "invalid byte sequence for encoding \"UTF8\": 0x00"
        ),
        WriteFailure::Refused(_)
    ));
}

#[test]
fn note_unrecorded_appears_in_health_with_session_and_time() {
    let session = uuid::Uuid::new_v4().to_string();
    let at = chrono::Utc::now();
    note_unrecorded(&session, at, "test reason");
    let found = health()
        .unrecorded
        .into_iter()
        .find(|u| u.session_id == session);
    assert_eq!(
        found.map(|u| (u.request_at, u.reason)),
        Some((at, "test reason".to_string()))
    );
}
