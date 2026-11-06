use super::*;
use crate::compress::manifest::sha256_hex;

#[tokio::test]
async fn original_result_recovery_survives_file_changes_and_rejects_wrong_hash()
-> anyhow::Result<()> {
    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let original = "first\r\n  α\t\"quoted\"\n";
    let body = serde_json::json!({"messages":[{"role":"user","content":[{
        "type":"tool_result","tool_use_id":"read-one","content":original
    }]}]});
    let mut req = tests_01::sample_request();
    req.request_body = Some(Bytes::from(serde_json::to_vec(&body)?));
    state.store.write().await.insert(req);
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("file.txt"), "changed after capture")?;
    let params = BodyPointerParams {
        tool_use_id: Some("read-one".into()),
        sha256: Some(sha256_hex(original.as_bytes())),
        ..BodyPointerParams::default()
    };
    let Json(result) = get_request_body(State(state.clone()), Path(42), Query(params))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let text: String = serde_json::from_str(
        result
            .text
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("missing text"))?,
    )?;
    assert_eq!(text, original);
    assert_eq!(result.pointer_resolved, Some(true));
    let wrong = BodyPointerParams {
        tool_use_id: Some("read-one".into()),
        sha256: Some("0".repeat(64)),
        ..BodyPointerParams::default()
    };
    assert!(matches!(
        get_request_body(State(state.clone()), Path(42), Query(wrong)).await,
        Err(StatusCode::CONFLICT)
    ));
    let incomplete = BodyPointerParams {
        tool_use_id: Some("read-one".into()),
        ..BodyPointerParams::default()
    };
    assert!(matches!(
        get_request_body(State(state), Path(42), Query(incomplete)).await,
        Err(StatusCode::BAD_REQUEST)
    ));
    Ok(())
}
