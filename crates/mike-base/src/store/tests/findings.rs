use super::diagnostic_stats::mk_diag;
use super::*;

fn assert_unfiltered_sorted_by_created_at_descending(store: &RequestStore, now: DateTime<Utc>) {
    let all = store.findings(None, None);
    assert_eq!(all.len(), 4, "four findings total: {all:?}");
    assert_eq!(all[0].diagnostic.created_at, now);
    assert_eq!(all[0].request_id, RequestId::from_raw(1));
    assert_eq!(
        all[1].diagnostic.created_at,
        now - chrono::Duration::minutes(1)
    );
    assert_eq!(all[1].request_id, RequestId::from_raw(3));
    assert_eq!(
        all[2].diagnostic.created_at,
        now - chrono::Duration::minutes(5)
    );
    assert_eq!(all[2].request_id, RequestId::from_raw(2));
    assert_eq!(
        all[3].diagnostic.created_at,
        now - chrono::Duration::minutes(10)
    );
    assert_eq!(all[3].request_id, RequestId::from_raw(1));
}

fn assert_unfiltered_finding_ref_is_populated_from_owning_request(store: &RequestStore) {
    let all = store.findings(None, None);
    assert_eq!(all[0].session_id, "sess-a");
    assert_eq!(all[0].request.status, "Complete");
    assert_eq!(all[0].request.path, "/v1/messages");
    assert_eq!(all[0].request.human_prompt, "hello");
}

fn assert_category_filter_narrows_to_justifying_shortcuts(
    store: &RequestStore,
    now: DateTime<Utc>,
) {
    let js = store.findings(Some(&DiagnosticCategory::JustifyingShortcuts), None);
    assert_eq!(js.len(), 2, "two JustifyingShortcuts findings: {js:?}");
    assert_eq!(js[0].diagnostic.created_at, now);
    assert_eq!(
        js[1].diagnostic.created_at,
        now - chrono::Duration::minutes(1)
    );
}

fn assert_analyzer_filter_narrows_to_overselling(store: &RequestStore) {
    let over = store.findings(None, Some("overselling_analyzer"));
    assert_eq!(over.len(), 1, "one overselling finding: {over:?}");
    assert_eq!(over[0].request_id, RequestId::from_raw(1));
}

fn assert_combined_filter_ands_category_and_analyzer(store: &RequestStore) {
    let both_hit = store.findings(
        Some(&DiagnosticCategory::JustifyingShortcuts),
        Some("justifying_shortcuts_analyzer"),
    );
    assert_eq!(both_hit.len(), 2, "matching combined filter: {both_hit:?}");

    let both_miss = store.findings(
        Some(&DiagnosticCategory::JustifyingShortcuts),
        Some("overselling_analyzer"),
    );
    assert!(
        both_miss.is_empty(),
        "non-matching combined filter must return empty: {both_miss:?}"
    );
}

fn assert_unknown_analyzer_returns_empty(store: &RequestStore) {
    let unknown = store.findings(None, Some("no_such_analyzer"));
    assert!(unknown.is_empty(), "unknown analyzer → empty: {unknown:?}");
}

#[test]
fn findings_filters_and_sorts_by_created_at_descending() -> anyhow::Result<()> {
    let now = Utc::now();
    let mut store = RequestStore::new(small_policy(10));

    let mut req1 = make_test_request(Some(RequestId::from_raw(1)), "sess-a");
    req1.diagnostics = vec![
        mk_diag(
            DiagnosticCategory::JustifyingShortcuts,
            "justifying_shortcuts_analyzer",
            now,
        ),
        mk_diag(
            DiagnosticCategory::Overselling,
            "overselling_analyzer",
            now - chrono::Duration::minutes(10),
        ),
    ];
    store.insert(req1);

    let mut req2 = make_test_request(Some(RequestId::from_raw(2)), "sess-b");
    req2.diagnostics = vec![mk_diag(
        DiagnosticCategory::DeflectingBlame,
        "deflecting_blame_analyzer",
        now - chrono::Duration::minutes(5),
    )];
    store.insert(req2);

    let mut req3 = make_test_request(Some(RequestId::from_raw(3)), "sess-a");
    req3.diagnostics = vec![mk_diag(
        DiagnosticCategory::JustifyingShortcuts,
        "justifying_shortcuts_analyzer",
        now - chrono::Duration::minutes(1),
    )];
    store.insert(req3);

    assert_unfiltered_sorted_by_created_at_descending(&store, now);
    assert_unfiltered_finding_ref_is_populated_from_owning_request(&store);
    assert_category_filter_narrows_to_justifying_shortcuts(&store, now);
    assert_analyzer_filter_narrows_to_overselling(&store);
    assert_combined_filter_ands_category_and_analyzer(&store);
    assert_unknown_analyzer_returns_empty(&store);

    Ok(())
}

#[test]
fn findings_empty_store_returns_empty() {
    let store = RequestStore::new(small_policy(10));
    assert!(store.findings(None, None).is_empty());
    assert!(
        store
            .findings(Some(&DiagnosticCategory::Overselling), None)
            .is_empty()
    );
    assert!(store.findings(None, Some("anything")).is_empty());
}

#[test]
fn latest_findings_outlive_the_request_entry_until_a_clean_analysis() {
    let mut store = RequestStore::new(small_policy(10));
    let diag = mk_diag(
        DiagnosticCategory::JustifyingShortcuts,
        "justifying_shortcuts_analyzer",
        Utc::now(),
    );
    store.record_latest_findings("sess-f", RequestId::from_raw(7), &[diag]);
    let Some((request_id, findings)) = store.latest_findings("sess-f") else {
        panic!("findings recorded for sess-f must be returned");
    };
    assert_eq!(request_id, RequestId::from_raw(7));
    assert_eq!(findings.len(), 1);
    assert!(store.latest_findings("sess-other").is_none());

    store.record_latest_findings("sess-f", RequestId::from_raw(8), &[]);
    assert!(store.latest_findings("sess-f").is_none());
}
