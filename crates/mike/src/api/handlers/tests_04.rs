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
