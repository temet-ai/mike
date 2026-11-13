use super::tests_01::*;
use super::*;

#[test]
pub(crate) fn findings_params_empty_object_is_all_none() -> anyhow::Result<()> {
    let params: FindingsParams = serde_json::from_value(serde_json::json!({}))?;
    assert!(params.category.is_none());
    assert!(params.analyzer.is_none());
    assert!(params.order.is_none());
    Ok(())
}

#[test]
pub(crate) fn sort_order_parses_asc_and_desc() -> anyhow::Result<()> {
    let asc: SortOrder = serde_json::from_value(serde_json::json!("asc"))?;
    let desc: SortOrder = serde_json::from_value(serde_json::json!("desc"))?;
    assert_eq!(asc, SortOrder::Asc);
    assert_eq!(desc, SortOrder::Desc);
    assert_eq!(asc.sql(), "ASC");
    assert_eq!(desc.sql(), "DESC");
    assert_eq!(asc.sql_reversed(), "DESC");
    assert_eq!(desc.sql_reversed(), "ASC");
    Ok(())
}

#[test]
pub(crate) fn sort_order_defaults_to_desc() {
    assert_eq!(SortOrder::default(), SortOrder::Desc);
}

#[test]
pub(crate) fn sort_order_rejects_unknown_and_uppercase_values() {
    for bad in ["ASC", "DESC", "ascending", "newest", ""] {
        let parsed: Result<SortOrder, _> = serde_json::from_value(serde_json::json!(bad));
        assert!(parsed.is_err(), "`{bad}` must fail SortOrder parsing");
    }
    let params: Result<ListParams, _> = serde_json::from_value(serde_json::json!({
        "order": "sideways"
    }));
    assert!(
        params.is_err(),
        "an invalid `order` must fail ListParams deserialization (→ 400 via axum Query)"
    );
}

pub(crate) async fn state_with_compressible_request(
    id: u64,
    real_input_tokens: Option<u32>,
) -> anyhow::Result<AppState> {
    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let big_json = serde_json::to_string_pretty(&serde_json::json!({
        "rows": (0..40).map(|i| serde_json::json!({"id": i, "name": format!("row-{i}")}))
            .collect::<Vec<_>>()
    }))?;
    let body = serde_json::to_vec(&serde_json::json!({
        "model": "m",
        "messages": [{
            "role": "user",
            "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": big_json}
            ]
        }]
    }))?;
    let mut req = sample_request();
    req.id = RequestId::from_raw(id);
    req.request_body = Some(Bytes::from(body));
    req.token_usage = real_input_tokens.map(|t| TokenUsage {
        input_tokens: t,
        output_tokens: 0,
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
    });
    {
        let mut store = state.store.write().await;
        store.insert(req);
    }
    Ok(state)
}

pub(crate) async fn state_with_file_read_request(
    id: u64,
    real_input_tokens: Option<u32>,
) -> anyhow::Result<AppState> {
    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let file_content: String = (1..=200)
        .map(|i| format!("line {i} of the file with enough padding to matter\n"))
        .collect();
    let body = serde_json::to_vec(&serde_json::json!({
        "model": "m",
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/big.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": file_content}
            ]},
            {"role": "assistant", "content": [{"type": "text", "text": "noted"}]},
            {"role": "user", "content": [{"type": "text", "text": "continue"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "noted"}]},
            {"role": "user", "content": [{"type": "text", "text": "continue"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "noted"}]},
            {"role": "user", "content": [{"type": "text", "text": "continue"}]}
        ]
    }))?;
    let mut req = sample_request();
    req.id = RequestId::from_raw(id);
    req.request_body = Some(Bytes::from(body));
    req.token_usage = real_input_tokens.map(|t| TokenUsage {
        input_tokens: t,
        output_tokens: 0,
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
    });
    {
        let mut store = state.store.write().await;
        store.insert(req);
    }
    Ok(state)
}

#[tokio::test]
pub(crate) async fn compress_estimate_reduces_file_read_and_reports_full_bodies_and_manifest()
-> anyhow::Result<()> {
    let state = state_with_file_read_request(7, Some(1000)).await?;
    let Json(resp) = compress_estimate(
        State(state),
        Json(CompressEstimateRequest { request_id: 7 }),
    )
    .await
    .map_err(|s| anyhow::anyhow!("handler returned {s}"))?;

    assert!(resp.compressed, "the large Read result must reduce");
    assert_eq!(resp.request_id, 7);
    assert!(resp.report.saved_bytes > 0, "bytes must be saved");
    assert!(resp.report.compressed_bytes < resp.report.original_bytes);
    assert_eq!(
        resp.report.token_basis,
        crate::compress::measure::TokenBasis::RealUsageRatio
    );
    assert_eq!(resp.report.est_original_tokens, 1000);
    assert!(resp.report.est_compressed_tokens < 1000);

    assert_eq!(resp.original_body.len(), resp.report.original_bytes);
    assert_eq!(resp.sent_body.len(), resp.report.compressed_bytes);
    assert!(resp.original_body.contains("line 1 of the file"));
    assert!(resp.sent_body.len() < resp.original_body.len());
    assert!(resp.sent_body.contains("[[mike:file_read_reduced]]"));

    assert_eq!(resp.manifest.stages.len(), 1);
    let stage = &resp.manifest.stages[0];
    assert_eq!(stage.stage, "compression.file_read");
    assert_eq!(stage.outcome, crate::wire::manifest::StageOutcome::Applied);
    assert_eq!(stage.reason_code, "applied");
    assert_eq!(stage.targets.len(), 1);
    assert_eq!(
        stage.targets[0].outcome,
        crate::wire::manifest::StageOutcome::Applied
    );
    assert_eq!(stage.targets[0].reason_code, "file_read_reduced");
    assert!(resp.manifest.verify_chain().is_ok());

    assert!(resp.diff.contains("--- original"), "diff header present");
    assert!(resp.diff.contains("+++ compressed"));
    assert!(!resp.diff.contains("[truncated]"));
    Ok(())
}

#[tokio::test]
pub(crate) async fn compress_estimate_unknown_id_is_404() -> anyhow::Result<()> {
    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let err = compress_estimate(
        State(state),
        Json(CompressEstimateRequest {
            request_id: 999_999,
        }),
    )
    .await
    .err()
    .ok_or_else(|| anyhow::anyhow!("expected an error status"))?;
    assert_eq!(err, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
pub(crate) async fn compress_estimate_no_body_is_zero_save_and_chain_closes() -> anyhow::Result<()>
{
    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let mut req = sample_request();
    req.id = RequestId::from_raw(10);
    req.request_body = None;
    {
        let mut store = state.store.write().await;
        store.insert(req);
    }
    let Json(resp) = compress_estimate(
        State(state),
        Json(CompressEstimateRequest { request_id: 10 }),
    )
    .await
    .map_err(|s| anyhow::anyhow!("handler returned {s}"))?;
    assert!(!resp.compressed);
    assert_eq!(resp.original_body, "");
    assert_eq!(resp.sent_body, "");
    assert!(resp.manifest.stages.is_empty());
    assert!(resp.manifest.verify_chain().is_ok());
    assert_eq!(resp.diff, "");
    Ok(())
}

#[tokio::test]
pub(crate) async fn compress_estimate_heuristic_when_no_token_usage() -> anyhow::Result<()> {
    let state = state_with_file_read_request(8, None).await?;
    let Json(resp) = compress_estimate(
        State(state),
        Json(CompressEstimateRequest { request_id: 8 }),
    )
    .await
    .map_err(|s| anyhow::anyhow!("handler returned {s}"))?;
    assert_eq!(
        resp.report.token_basis,
        crate::compress::measure::TokenBasis::HeuristicBytesDiv4
    );
    assert_eq!(
        resp.report.est_original_tokens,
        (resp.report.original_bytes / 4) as u64
    );
    Ok(())
}

#[tokio::test]
pub(crate) async fn compress_stats_aggregates_over_recent_requests() -> anyhow::Result<()> {
    let state = state_with_compressible_request(9, Some(500)).await?;
    let Json(resp) = compress_stats(State(state), Query(CompressStatsQuery { limit: Some(10) }))
        .await
        .map_err(|s| anyhow::anyhow!("handler returned {s}"))?;

    assert_eq!(resp.requests_scanned, 1);
    assert_eq!(resp.requests_compressed, 1);
    assert!(resp.total_original_bytes > resp.total_compressed_bytes);
    assert_eq!(resp.total_original_tokens, 500);
    assert!(resp.total_est_compressed_tokens < 500);
    assert!(resp.total_saved_pct > 0.0);
    Ok(())
}

#[tokio::test]
pub(crate) async fn compress_stats_empty_store_is_zeroed() -> anyhow::Result<()> {
    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let Json(resp) = compress_stats(State(state), Query(CompressStatsQuery { limit: None }))
        .await
        .map_err(|s| anyhow::anyhow!("handler returned {s}"))?;
    assert_eq!(resp.requests_scanned, 0);
    assert_eq!(resp.requests_compressed, 0);
    assert_eq!(resp.total_saved_pct, 0.0);
    Ok(())
}

#[test]
pub(crate) fn validate_blocks_input_rejects_empty_with_400() {
    let Err((status, body)) = validate_blocks_input(&[]) else {
        panic!("empty blocks must reject with Err, got Ok");
    };
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body.0.error.contains("blocks"),
        "400 body must name the blocks field; got {:?}",
        body.0.error
    );
}

#[test]
pub(crate) fn validate_blocks_input_accepts_one_block() -> anyhow::Result<()> {
    let blocks = vec![PromptCompositionBlockInput {
        text: "b".to_string(),
        cache_control: None,
    }];
    let value = validate_blocks_input(&blocks).map_err(|(status, body)| {
        anyhow::anyhow!("validation rejected: {status} {}", body.0.error)
    })?;
    let arr = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("blocks did not normalize to an array"))?;
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["type"], "text");
    assert_eq!(arr[0]["text"], "b");
    Ok(())
}

#[test]
pub(crate) fn parse_mode_input_rejects_unknown_with_400_enumerating_modes() {
    let Err((status, body)) = parse_mode_input("totally-invalid") else {
        panic!("unknown mode must reject with Err, got Ok");
    };
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err = &body.0.error;
    assert!(
        err.contains("observe") && err.contains("template") && err.contains("replace"),
        "400 body must enumerate the valid modes; got {err:?}"
    );
}

#[test]
pub(crate) fn parse_supported_mode_input_rejects_replace_in_v1() {
    let Err((status, body)) = parse_supported_mode_input("replace") else {
        panic!("replace is not supported in V1, expected Err, got Ok");
    };
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body.0.error.contains("replace mode is not supported"),
        "400 body must cite the V1 replace rejection; got {:?}",
        body.0.error
    );
}

#[test]
pub(crate) fn composition_patch_is_empty_true_for_all_none() {
    assert!(composition_patch_is_empty(
        &PromptCompositionPatch::default()
    ));
}

#[test]
pub(crate) fn composition_patch_is_empty_false_when_any_field_present() {
    let with_label = PromptCompositionPatch {
        label: Some("x".to_string()),
        ..PromptCompositionPatch::default()
    };
    assert!(!composition_patch_is_empty(&with_label));
    let with_mode = PromptCompositionPatch {
        mode: Some("template".to_string()),
        ..PromptCompositionPatch::default()
    };
    assert!(!composition_patch_is_empty(&with_mode));
}

#[tokio::test]
pub(crate) async fn get_sync_status_returns_default_snapshot() -> anyhow::Result<()> {
    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let Json(status) = get_sync_status(State(state)).await;

    assert!(
        status.last_run_at.is_none(),
        "last_run_at must be None before any tick"
    );
    assert!(
        !status.dest_reachable,
        "dest_reachable must default false before any tick"
    );
    assert!(
        status.last_report.tables.is_empty(),
        "last_report.tables must be empty before any successful drain"
    );
    assert_eq!(
        status.last_report.total_rows, 0,
        "last_report.total_rows must be 0 before any successful drain"
    );
    assert!(
        status.last_error.is_none(),
        "last_error must be None before any failed tick"
    );

    let json = serde_json::to_value(&status)?;
    assert!(json.get("last_run_at").is_some());
    assert!(json.get("dest_reachable").is_some());
    assert!(json.get("last_report").is_some());
    assert!(json.get("last_error").is_some());
    assert!(
        json["last_report"].get("tables").is_some(),
        "last_report must carry a tables array"
    );
    assert!(
        json["last_report"].get("total_rows").is_some(),
        "last_report must carry total_rows"
    );
    Ok(())
}
