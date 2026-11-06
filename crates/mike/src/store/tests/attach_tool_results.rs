use super::*;

fn make_tool_result(
    tool_use_id: &str,
    is_error: bool,
    content: serde_json::Value,
) -> crate::store::models::ToolResult {
    let bytes = serde_json::to_vec(&content).unwrap_or_default();
    let sha = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(&bytes);
        hex::encode(h.finalize())
    };
    crate::store::models::ToolResult {
        tool_use_id: tool_use_id.to_string(),
        is_error,
        content_sha256: sha,
        content_bytes: bytes.len() as u64,
        content,
    }
}

#[test]
fn attach_tool_results_routes_each_block_to_its_emitting_request() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));

    let mut r1 = make_test_request(Some(RequestId::from_raw(301)), "sess-routes");
    r1.tool_calls.push(crate::types::ToolCall {
        tool_id: "toolu_a".to_string(),
        name: "Bash".to_string(),
        input_json: "{}".to_string(),
        complete: true,
    });
    store.insert(r1);

    let mut r2 = make_test_request(Some(RequestId::from_raw(302)), "sess-routes");
    r2.tool_calls.push(crate::types::ToolCall {
        tool_id: "toolu_b".to_string(),
        name: "Edit".to_string(),
        input_json: "{}".to_string(),
        complete: true,
    });
    store.insert(r2);

    let outcome = store.attach_tool_results(
        "sess-routes",
        vec![
            make_tool_result("toolu_a", false, serde_json::json!("result a")),
            make_tool_result("toolu_b", true, serde_json::json!("error b")),
        ],
    );

    assert_eq!(outcome.touched.len(), 2);
    assert!(outcome.db_fallback_candidates.is_empty());
    let touched_raw: Vec<u64> = outcome.touched.iter().map(|id| id.as_u64()).collect();
    assert!(touched_raw.contains(&301));
    assert!(touched_raw.contains(&302));

    let r1_after = store
        .get(&RequestId::from_raw(301))
        .ok_or_else(|| anyhow::anyhow!("r1 missing"))?;
    assert_eq!(r1_after.tool_results.len(), 1);
    assert_eq!(r1_after.tool_results[0].tool_use_id, "toolu_a");
    assert!(!r1_after.tool_results[0].is_error);

    let r2_after = store
        .get(&RequestId::from_raw(302))
        .ok_or_else(|| anyhow::anyhow!("r2 missing"))?;
    assert_eq!(r2_after.tool_results.len(), 1);
    assert_eq!(r2_after.tool_results[0].tool_use_id, "toolu_b");
    assert!(r2_after.tool_results[0].is_error);
    Ok(())
}

#[test]
fn attach_tool_results_surfaces_unmatched_results_for_db_fallback() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));
    let mut r1 = make_test_request(Some(RequestId::from_raw(401)), "sess-orphan");
    r1.tool_calls.push(crate::types::ToolCall {
        tool_id: "toolu_known".to_string(),
        name: "Bash".to_string(),
        input_json: "{}".to_string(),
        complete: true,
    });
    store.insert(r1);

    let outcome = store.attach_tool_results(
        "sess-orphan",
        vec![
            make_tool_result("toolu_unknown", false, serde_json::json!("ghost")),
            make_tool_result("toolu_known", false, serde_json::json!("real")),
        ],
    );
    assert_eq!(outcome.touched.len(), 1);
    assert_eq!(outcome.touched[0].as_u64(), 401);
    assert_eq!(outcome.db_fallback_candidates.len(), 1);
    assert_eq!(
        outcome.db_fallback_candidates[0].tool_use_id, "toolu_unknown",
        "unmatched id must be handed back to the caller for DB resolution, not dropped"
    );

    let r1_after = store
        .get(&RequestId::from_raw(401))
        .ok_or_else(|| anyhow::anyhow!("r1 missing"))?;
    assert_eq!(r1_after.tool_results.len(), 1);
    assert_eq!(r1_after.tool_results[0].tool_use_id, "toolu_known");
    Ok(())
}

#[test]
fn attach_tool_results_idempotent_when_same_content_arrives_twice() -> anyhow::Result<()> {
    let mut store = RequestStore::new(small_policy(10));
    let mut r1 = make_test_request(Some(RequestId::from_raw(501)), "sess-dedupe");
    r1.tool_calls.push(crate::types::ToolCall {
        tool_id: "toolu_x".to_string(),
        name: "Bash".to_string(),
        input_json: "{}".to_string(),
        complete: true,
    });
    store.insert(r1);

    let first = store.attach_tool_results(
        "sess-dedupe",
        vec![make_tool_result(
            "toolu_x",
            false,
            serde_json::json!("the same content"),
        )],
    );
    assert_eq!(first.touched.len(), 1);
    assert!(first.db_fallback_candidates.is_empty());

    let second = store.attach_tool_results(
        "sess-dedupe",
        vec![make_tool_result(
            "toolu_x",
            false,
            serde_json::json!("the same content"),
        )],
    );
    assert!(
        second.touched.is_empty(),
        "duplicate result must be detected and skipped (no row update needed)"
    );
    assert!(
        second.db_fallback_candidates.is_empty(),
        "in-memory match (even when dedup-skipped) must NOT spill to DB-fallback"
    );
    let r1_after = store
        .get(&RequestId::from_raw(501))
        .ok_or_else(|| anyhow::anyhow!("r1 missing"))?;
    assert_eq!(r1_after.tool_results.len(), 1);
    Ok(())
}

#[test]
fn attach_tool_results_empty_results_is_noop() {
    let mut store = RequestStore::new(small_policy(10));
    let outcome = store.attach_tool_results("any-session", vec![]);
    assert!(outcome.touched.is_empty());
    assert!(outcome.db_fallback_candidates.is_empty());
}

#[test]
fn attach_tool_results_unknown_session_surfaces_db_fallback() {
    let mut store = RequestStore::new(small_policy(10));
    let outcome = store.attach_tool_results(
        "ghost-session",
        vec![make_tool_result(
            "toolu_x",
            false,
            serde_json::json!("orphan"),
        )],
    );
    assert!(outcome.touched.is_empty());
    assert_eq!(outcome.db_fallback_candidates.len(), 1);
    assert_eq!(outcome.db_fallback_candidates[0].tool_use_id, "toolu_x");
}
