use super::*;

pub(super) fn mk_diag(
    category: DiagnosticCategory,
    analyzer: &str,
    created_at: chrono::DateTime<Utc>,
) -> Diagnostic {
    Diagnostic {
        category,
        analyzer: analyzer.to_string(),
        advice: "advice".to_string(),
        matched_text: "text".to_string(),
        match_range: (0, 4),
        other_hits: 0,
        created_at,
    }
}

#[test]
fn diagnostic_stats_walks_entries_live_including_last_24h_filter() -> anyhow::Result<()> {
    let now = Utc::now();
    let old = now - chrono::Duration::hours(48);

    let mut store = RequestStore::new(small_policy(10));

    let mut req1 = make_test_request(None, "sess-a");
    req1.diagnostics = vec![
        mk_diag(
            DiagnosticCategory::JustifyingShortcuts,
            "justifying_shortcuts_analyzer",
            now,
        ),
        mk_diag(DiagnosticCategory::Overselling, "overselling_analyzer", now),
    ];
    store.insert(req1);

    let mut req2 = make_test_request(None, "sess-a");
    req2.diagnostics = vec![mk_diag(
        DiagnosticCategory::JustifyingShortcuts,
        "justifying_shortcuts_analyzer",
        now - chrono::Duration::minutes(5),
    )];
    store.insert(req2);

    let mut req3 = make_test_request(None, "sess-b");
    req3.diagnostics = vec![mk_diag(
        DiagnosticCategory::DeflectingBlame,
        "deflecting_blame_analyzer",
        old,
    )];
    store.insert(req3);

    let stats = store.diagnostic_stats(now);

    assert_eq!(
        stats
            .totals_by_category
            .get(&DiagnosticCategory::JustifyingShortcuts),
        Some(&2)
    );
    assert_eq!(
        stats
            .totals_by_category
            .get(&DiagnosticCategory::Overselling),
        Some(&1)
    );
    assert_eq!(
        stats
            .totals_by_category
            .get(&DiagnosticCategory::DeflectingBlame),
        Some(&1)
    );
    assert_eq!(
        stats
            .totals_by_analyzer
            .get("justifying_shortcuts_analyzer"),
        Some(&2)
    );
    assert_eq!(
        stats.totals_by_analyzer.get("overselling_analyzer"),
        Some(&1)
    );
    assert_eq!(
        stats.totals_by_analyzer.get("deflecting_blame_analyzer"),
        Some(&1)
    );

    assert_eq!(stats.last_24h, 3, "only findings within 24h count");

    Ok(())
}

#[test]
fn diagnostic_stats_empty_store_returns_empty() {
    let store = RequestStore::new(small_policy(10));
    let stats = store.diagnostic_stats(Utc::now());
    assert!(stats.totals_by_category.is_empty());
    assert!(stats.totals_by_analyzer.is_empty());
    assert_eq!(stats.last_24h, 0);
}
