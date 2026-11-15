use super::*;

fn embedder(key: &str) -> EmbedderRow {
    EmbedderRow {
        id: 2,
        embedder_key: key.to_owned(),
        model: "embeddinggemma".to_owned(),
        dim: 768,
        chunk_max_chars: 1024,
        prefix_version: "doc-prefix-v1".to_owned(),
        urls_seen: Vec::new(),
        canary_version: "v1".to_owned(),
        canary_vectors: serde_json::json!({}),
        status: "current".to_owned(),
        first_seen_at: Utc::now(),
        last_checked_at: Utc::now(),
        copy_valid_from: None,
        copy_verify: None,
    }
}

fn source() -> AnchorSource {
    AnchorSource {
        request_id: Some(41),
        origin_session_uuid: Some("6f1c2a52-6a0e-4b8e-9f43-1c1b8d7c2e10".to_owned()),
        origin_turn_id: Some(7),
        source_ref: "mirror:claims-without-proof:7".to_owned(),
        pattern: "claims-without-proof".to_owned(),
        text: "you said it passed, show me the run".to_owned(),
        data: serde_json::json!({ "text_source": "request" }),
    }
}

#[test]
fn carried_anchor_under_new_embedder_keeps_pattern_and_origin() {
    let item = carried(&source(), &embedder("gemma-key"), vec![0.5, 0.5]);
    assert_eq!(item.kind, "anchor");
    assert_eq!(item.embedder_key, "gemma-key");
    assert_eq!(item.pattern.as_deref(), Some("claims-without-proof"));
    assert_eq!(item.source_ref, "mirror:claims-without-proof:7");
    assert_eq!(item.request_id, Some(41));
    assert_eq!(item.origin_turn_id, Some(7));
    assert_eq!(item.session_id, None);
    assert_eq!(item.operator_turn_id, None);
    assert_eq!(item.embedding, vec![0.5, 0.5]);
    assert_eq!(item.data["text_source"], "request");
}
