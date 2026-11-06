use super::tests_01::*;
use super::tests_02::*;
use super::*;

#[test]
pub(crate) fn session_summary_parity_live_vs_db_fallback() -> anyhow::Result<()> {
    use std::collections::HashSet;

    let view = sample_session_view();
    let row = sample_session_summary_row_matching_view();

    let from_live = serde_json::to_value(SessionSummary::from(&view))?;
    let from_db = serde_json::to_value(SessionSummary::from(&row))?;

    let live_obj = from_live
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("live summary must serialise to object"))?;
    let db_obj = from_db
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("db summary must serialise to object"))?;

    let omitted: HashSet<&str> = SessionSummary::FALLBACK_OMITTED_FIELDS
        .iter()
        .copied()
        .collect();

    let live_keys: HashSet<&str> = live_obj.keys().map(String::as_str).collect();
    let db_keys: HashSet<&str> = db_obj.keys().map(String::as_str).collect();
    assert_eq!(
        live_keys, db_keys,
        "live and DB-fallback must produce the same JSON key set"
    );

    for key in live_keys.iter().copied() {
        if omitted.contains(key) {
            continue;
        }
        assert_eq!(
            live_obj.get(key),
            db_obj.get(key),
            "parity mismatch on field `{key}`: live={:?} db={:?}",
            live_obj.get(key),
            db_obj.get(key),
        );
    }

    for key in SessionSummary::FALLBACK_OMITTED_FIELDS {
        assert!(
            db_obj.contains_key(*key),
            "omitted field `{key}` must still appear in DB-fallback JSON \
             (as its default), not vanish"
        );
        assert!(
            live_obj.contains_key(*key),
            "omitted field `{key}` must still appear in live JSON"
        );
    }

    assert_eq!(db_obj.get("ttl_override"), Some(&Value::Null));

    Ok(())
}

pub(crate) fn sample_diagnostic() -> Diagnostic {
    Diagnostic {
        category: DiagnosticCategory::JustifyingShortcuts,
        analyzer: "justifying_shortcuts_analyzer".to_string(),
        advice: "not a licence to reduce scope".to_string(),
        matched_text: "pragmatic".to_string(),
        match_range: (10, 19),
        other_hits: 0,
        created_at: fixed_time(2026, 4, 11, 12, 0),
    }
}

#[test]
pub(crate) fn diagnostic_dto_serializes_with_expected_keys() -> anyhow::Result<()> {
    use std::collections::HashSet;

    let dto = DiagnosticDto::from(&sample_diagnostic());
    let value = serde_json::to_value(&dto)?;
    let map = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("DiagnosticDto must serialize to a JSON object"))?;
    let actual: HashSet<String> = map.keys().cloned().collect();
    let expected: HashSet<String> = [
        "category",
        "analyzer",
        "advice",
        "matched_text",
        "match_range",
        "other_hits",
        "created_at",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    assert_eq!(
        actual, expected,
        "DiagnosticDto JSON keys drift: got {actual:?}, want {expected:?}"
    );

    assert_eq!(
        map.get("category"),
        Some(&Value::String("justifying_shortcuts".to_string())),
        "category must serialize as snake_case enum discriminator"
    );
    Ok(())
}

#[test]
pub(crate) fn diagnostic_stats_serializes_with_expected_keys() -> anyhow::Result<()> {
    use std::collections::HashSet;

    let mut totals_by_category = BTreeMap::new();
    totals_by_category.insert(DiagnosticCategory::JustifyingShortcuts, 2usize);
    totals_by_category.insert(DiagnosticCategory::Overselling, 1usize);

    let mut totals_by_analyzer = BTreeMap::new();
    totals_by_analyzer.insert("justifying_shortcuts_analyzer".to_string(), 2usize);
    totals_by_analyzer.insert("overselling_analyzer".to_string(), 1usize);

    let stats = DiagnosticStats {
        totals_by_category,
        totals_by_analyzer,
        last_24h: 3,
    };
    let value = serde_json::to_value(&stats)?;
    let map = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("DiagnosticStats must serialize to a JSON object"))?;
    let actual: HashSet<String> = map.keys().cloned().collect();
    let expected: HashSet<String> = ["totals_by_category", "totals_by_analyzer", "last_24h"]
        .into_iter()
        .map(str::to_string)
        .collect();
    assert_eq!(
        actual, expected,
        "DiagnosticStats JSON keys drift: got {actual:?}, want {expected:?}"
    );

    let by_cat = map
        .get("totals_by_category")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("totals_by_category must be an object"))?;
    assert_eq!(by_cat.get("justifying_shortcuts"), Some(&Value::from(2u64)));
    assert_eq!(by_cat.get("overselling"), Some(&Value::from(1u64)));
    assert_eq!(map.get("last_24h"), Some(&Value::from(3u64)));
    Ok(())
}

#[test]
pub(crate) fn request_summary_json_snapshot() {
    let summary = RequestSummary::from(&sample_request());
    insta::assert_json_snapshot!(summary);
}

#[test]
pub(crate) fn request_detail_json_snapshot() {
    let detail = RequestDetail::from_request(&sample_request(), &empty_composer_cache(), None);
    insta::assert_json_snapshot!(detail);
}

#[test]
pub(crate) fn diagnostic_stats_json_snapshot() -> anyhow::Result<()> {
    let mut totals_by_category = BTreeMap::new();
    totals_by_category.insert(DiagnosticCategory::JustifyingShortcuts, 2usize);
    totals_by_category.insert(DiagnosticCategory::Overselling, 1usize);
    let mut totals_by_analyzer = BTreeMap::new();
    totals_by_analyzer.insert("justifying_shortcuts_analyzer".to_string(), 2usize);
    totals_by_analyzer.insert("overselling_analyzer".to_string(), 1usize);
    let stats = DiagnosticStats {
        totals_by_category,
        totals_by_analyzer,
        last_24h: 3,
    };
    let value = serde_json::to_value(&stats)?;
    insta::assert_json_snapshot!(value);
    Ok(())
}

#[tokio::test]
pub(crate) async fn diagnostics_patterns_route_returns_all_19_with_three_strict_entries()
-> anyhow::Result<()> {
    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let Json(response) = get_diagnostic_patterns(State(state)).await;
    assert_eq!(response.patterns.len(), 19);

    let strict_names: Vec<&str> = response
        .patterns
        .iter()
        .filter(|p| p.enforcement == crate::analysis::Enforcement::Strict)
        .map(|p| p.name)
        .collect();
    assert_eq!(
        strict_names.len(),
        3,
        "expected exactly 3 strict-enforcement patterns, got {strict_names:?}"
    );
    for expected in ["pre_existing", "out_of_scope", "natural_followup"] {
        assert!(
            strict_names.contains(&expected),
            "expected `{expected}` among strict patterns, got {strict_names:?}"
        );
    }
    Ok(())
}

#[tokio::test]
pub(crate) async fn diagnostics_patterns_route_reports_effective_enforcement_override()
-> anyhow::Result<()> {
    use crate::analysis::{AnalysisConfig, AnalyzerRegistry, Enforcement, PatternConfig};

    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;

    let mut cfg = AnalysisConfig::default();
    cfg.patterns.insert(
        "robust".to_string(),
        PatternConfig {
            advice: None,
            enforcement: Some(Enforcement::Strict),
        },
    );
    let registry = AnalyzerRegistry::with_defaults(&cfg)?;
    state
        .analyzer_registry()
        .store(std::sync::Arc::new(registry));

    let Json(response) = get_diagnostic_patterns(State(state)).await;
    let robust = response
        .patterns
        .iter()
        .find(|p| p.name == "robust")
        .ok_or_else(|| anyhow::anyhow!("robust pattern must be present"))?;
    assert_eq!(robust.enforcement, Enforcement::Strict);

    let exported: serde_json::Value = serde_json::from_str(&crate::analysis::corpus::render()?)?;
    let exported_robust = exported["patterns"]
        .as_array()
        .and_then(|arr| arr.iter().find(|p| p["name"] == "robust"))
        .ok_or_else(|| anyhow::anyhow!("robust pattern must be present in exported corpus"))?;
    assert_ne!(exported_robust["enforcement"], serde_json::json!("strict"));
    Ok(())
}

pub(crate) fn sample_finding_view() -> crate::store::FindingView {
    crate::store::FindingView {
        request_id: RequestId::from_raw(77),
        session_id: "sess-find".to_string(),
        diagnostic: sample_diagnostic(),
        request: crate::store::views::FindingRequestRef {
            model: Some("claude-sonnet-4-20250514".to_string()),
            status: "Complete".to_string(),
            path: "/v1/messages".to_string(),
            project: Some("ai/tools/proxy".to_string()),
            human_prompt: "Do the thing".to_string(),
            created_at: fixed_time(2026, 4, 11, 12, 0),
        },
    }
}

#[test]
pub(crate) fn post_session_message_body_accepts_from_aliases() {
    for key in ["from_session_id", "from", "from_session_uuid"] {
        let json = format!(r#"{{"{key}":"11111111-1111-1111-1111-111111111111","message":"hi"}}"#);
        let body: PostSessionMessageBody = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("key `{key}` should deserialize: {e}"));
        assert_eq!(
            body.from_session_id.as_deref(),
            Some("11111111-1111-1111-1111-111111111111"),
            "key `{key}` must populate from_session_id",
        );
        assert_eq!(body.message, "hi");
    }
    let body: PostSessionMessageBody = serde_json::from_str(r#"{"message":"hi"}"#)
        .unwrap_or_else(|e| panic!("no-sender body should deserialize: {e}"));
    assert!(body.from_session_id.is_none());
}

#[test]
pub(crate) fn finding_dto_serializes_with_expected_keys() -> anyhow::Result<()> {
    use std::collections::HashSet;

    let dto = FindingDto::from(&sample_finding_view());
    let value = serde_json::to_value(&dto)?;
    let map = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("FindingDto must serialize to a JSON object"))?;
    let actual: HashSet<String> = map.keys().cloned().collect();
    let expected: HashSet<String> = [
        "request_id",
        "session_id",
        "category",
        "analyzer",
        "advice",
        "matched_text",
        "match_range",
        "other_hits",
        "created_at",
        "request",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    assert_eq!(
        actual, expected,
        "FindingDto JSON keys drift: got {actual:?}, want {expected:?}"
    );

    assert_eq!(
        map.get("category"),
        Some(&Value::String("justifying_shortcuts".to_string())),
        "category must serialize as snake_case enum discriminator"
    );
    let inner = map
        .get("request")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("request must be an object: {value}"))?;
    let inner_actual: HashSet<String> = inner.keys().cloned().collect();
    let inner_expected: HashSet<String> = [
        "model",
        "status",
        "path",
        "project",
        "human_prompt",
        "created_at",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    assert_eq!(
        inner_actual, inner_expected,
        "FindingRequestRef keys drift: got {inner_actual:?}, want {inner_expected:?}"
    );
    Ok(())
}

#[test]
pub(crate) fn finding_dto_passes_full_text_through_and_copies_ids() -> anyhow::Result<()> {
    let mut view = sample_finding_view();
    view.request.human_prompt = "h".repeat(500);

    let dto = FindingDto::from(&view);
    assert_eq!(dto.request_id, 77);
    assert_eq!(dto.session_id, "sess-find");
    assert_eq!(dto.analyzer, "justifying_shortcuts_analyzer");
    assert!(dto.created_at.starts_with("2026-04-11T12:00:00"));
    assert_eq!(
        dto.request.human_prompt.len(),
        500,
        "human_prompt must NOT be truncated for display"
    );
    assert!(dto.request.created_at.starts_with("2026-04-11T12:00:00"));
    Ok(())
}

#[test]
pub(crate) fn findings_params_parses_valid_category() -> anyhow::Result<()> {
    let params: FindingsParams = serde_json::from_value(serde_json::json!({
        "category": "deflecting_blame"
    }))?;
    assert_eq!(params.category, Some(DiagnosticCategory::DeflectingBlame));
    assert!(params.analyzer.is_none());
    Ok(())
}

#[test]
pub(crate) fn findings_params_rejects_unknown_category() {
    let parsed: Result<FindingsParams, _> = serde_json::from_value(serde_json::json!({
        "category": "no_such_category"
    }));
    assert!(
        parsed.is_err(),
        "unknown category must fail serde deserialization"
    );
}

#[test]
pub(crate) fn findings_params_parses_both_filters() -> anyhow::Result<()> {
    let params: FindingsParams = serde_json::from_value(serde_json::json!({
        "category": "overselling",
        "analyzer": "overselling_analyzer"
    }))?;
    assert_eq!(params.category, Some(DiagnosticCategory::Overselling));
    assert_eq!(params.analyzer.as_deref(), Some("overselling_analyzer"));
    Ok(())
}
