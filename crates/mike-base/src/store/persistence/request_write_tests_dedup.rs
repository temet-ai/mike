#[cfg(test)]
mod tests {
    use sha2::Digest as _;

    use super::super::PersistHandle;
    use crate::store::persistence::request_write::IdMode;
    use crate::store::persistence::test_support as test_util;

    #[tokio::test]
    async fn insert_request_initial_rerun_keeps_single_predecessor_and_delta_set()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut predecessor = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(95_001)),
            &session_id,
        );
        predecessor.request_body = Some(bytes::Bytes::from(
            serde_json::json!({"messages": [{"role": "user", "content": "first"}]}).to_string(),
        ));
        handle
            .insert_request_initial(&predecessor, true, IdMode::Ensure)
            .await?;

        let mut current = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(95_002)),
            &session_id,
        );
        current.created_at = predecessor.created_at + chrono::Duration::seconds(1);
        current.request_body = Some(bytes::Bytes::from(
            serde_json::json!({"messages": [
                {"role": "user", "content": "first"},
                {"role": "assistant", "content": "second"}
            ]})
            .to_string(),
        ));

        handle
            .insert_request_initial(&current, false, IdMode::Ensure)
            .await?;
        handle
            .insert_request_initial(&current, false, IdMode::Ensure)
            .await?;

        let (predecessor_request_id,): (Option<i64>,) =
            sqlx::query_as("SELECT predecessor_request_id FROM requests.requests WHERE id = $1")
                .bind(95_002_i64)
                .fetch_one(&handle.pool)
                .await?;
        assert_eq!(
            predecessor_request_id,
            Some(95_001),
            "predecessor must be recorded on the request row"
        );

        let distinct_predecessors: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT predecessor_request_id) FROM requests.deltas WHERE request_id = $1",
        )
        .bind(95_002_i64)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(
            distinct_predecessors, 1,
            "R4/R9: a re-run must never leave change-rows against more than one predecessor"
        );

        let delta_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM requests.deltas WHERE request_id = $1")
                .bind(95_002_i64)
                .fetch_one(&handle.pool)
                .await?;
        assert_eq!(
            delta_rows, 1,
            "re-run must not duplicate the single 'added' change-row"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn insert_request_initial_identical_resend_still_records_predecessor()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut predecessor = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(96_001)),
            &session_id,
        );
        let body = serde_json::json!({"messages": [{"role": "user", "content": "same"}]});
        predecessor.request_body = Some(bytes::Bytes::from(body.to_string()));
        handle
            .insert_request_initial(&predecessor, true, IdMode::Ensure)
            .await?;

        let mut current = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(96_002)),
            &session_id,
        );
        current.created_at = predecessor.created_at + chrono::Duration::seconds(1);
        current.request_body = Some(bytes::Bytes::from(body.to_string()));
        handle
            .insert_request_initial(&current, false, IdMode::Ensure)
            .await?;

        let (predecessor_request_id,): (Option<i64>,) =
            sqlx::query_as("SELECT predecessor_request_id FROM requests.requests WHERE id = $1")
                .bind(96_002_i64)
                .fetch_one(&handle.pool)
                .await?;
        assert_eq!(
            predecessor_request_id,
            Some(96_001),
            "an identical re-send must still record its predecessor"
        );

        let delta_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM requests.deltas WHERE request_id = $1")
                .bind(96_002_i64)
                .fetch_one(&handle.pool)
                .await?;
        assert_eq!(
            delta_rows, 0,
            "an identical re-send has zero change-rows, not a missing predecessor"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    #[allow(
        clippy::expect_used,
        reason = "test fixture; a NULL here is the failure the test exists to catch"
    )]
    async fn insert_request_initial_records_sent_patch_and_reconstructs_byte_exact()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(97_001)),
            &session_id,
        );
        let system_block = "You are a helpful assistant. ".repeat(40);
        let client_body = serde_json::json!({
            "system": system_block,
            "messages": [
                {"role": "user", "content": "please read this large file for me"}
            ]
        });
        let client_bytes = bytes::Bytes::from(client_body.to_string());
        let sent_body = serde_json::json!({
            "system": system_block,
            "messages": [
                {"role": "user", "content": "please read this large file for me\n<system-reminder>injected note</system-reminder>"}
            ]
        });
        let sent_bytes = bytes::Bytes::from(sent_body.to_string());

        req.request_body = Some(client_bytes.clone());
        req.sent_body = Some(sent_bytes.clone());
        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;

        let (filtered,): (Option<Vec<u8>>,) =
            sqlx::query_as("SELECT filtered_request_body FROM requests.requests WHERE id = $1")
                .bind(97_001_i64)
                .fetch_one(&handle.pool)
                .await?;
        assert_eq!(
            filtered, None,
            "the sent-body storage fix means no full sent body is ever stored"
        );

        let (patch_start, patch_end, patch_bytes): (Option<i32>, Option<i32>, Option<Vec<u8>>) =
            sqlx::query_as(
                "SELECT sent_patch_start, sent_patch_end, sent_patch_bytes
                   FROM requests.deltas
                  WHERE request_id = $1 AND change_kind_id = 1",
            )
            .bind(97_001_i64)
            .fetch_one(&handle.pool)
            .await?;
        let patch_start = patch_start.expect("mutated message must record a sent patch");
        let patch_end = patch_end.expect("mutated message must record a sent patch");
        let patch_bytes = patch_bytes.expect("mutated message must record a sent patch");
        assert!(patch_end > patch_start);
        assert!(
            patch_bytes.len() < client_bytes.len(),
            "stored patch ({} bytes) must be on the order of the mutation, not the full body ({} bytes)",
            patch_bytes.len(),
            client_bytes.len()
        );

        let reconstructed = crate::store::persistence::rehydrate::fetch_request_sent_body_by_id(
            &handle.pool,
            97_001,
        )
        .await?
        .expect("reconstruction must return Some for a request with request_body");
        assert_eq!(
            reconstructed.as_ref(),
            sent_bytes.as_ref(),
            "reconstruction must reproduce the exact bytes forwarded upstream"
        );

        let expected_sha = hex::encode(sha2::Sha256::digest(sent_bytes.as_ref()));
        let actual_sha = hex::encode(sha2::Sha256::digest(reconstructed.as_ref()));
        assert_eq!(
            actual_sha, expected_sha,
            "reconstructed bytes must hash to the true sent sha256 (manifest self-check)"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    #[allow(
        clippy::expect_used,
        reason = "test fixture; a NULL here is the failure the test exists to catch"
    )]
    async fn insert_request_initial_reconstructs_combined_system_and_message_mutation()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(97_004)),
            &session_id,
        );
        let client_body = serde_json::json!({
            "system": [{"type": "text", "text": "OLD-SYSTEM-PROMPT"}],
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "please help"}]}
            ]
        });
        let client_bytes = bytes::Bytes::from(client_body.to_string());
        let sent_body = serde_json::json!({
            "system": [{"type": "text", "text": "NEW-SYSTEM-PROMPT-REWRITTEN"}],
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "please help"},
                    {"type": "text", "text": "<system-reminder>injected</system-reminder>"}
                ]}
            ]
        });
        let sent_bytes = bytes::Bytes::from(sent_body.to_string());

        req.request_body = Some(client_bytes.clone());
        req.sent_body = Some(sent_bytes.clone());
        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;

        let (envelope_start, envelope_end, envelope_bytes): (
            Option<i32>,
            Option<i32>,
            Option<Vec<u8>>,
        ) = sqlx::query_as(
            "SELECT sent_patch_start, sent_patch_end, sent_patch_bytes
               FROM requests.requests
              WHERE id = $1",
        )
        .bind(97_004_i64)
        .fetch_one(&handle.pool)
        .await?;
        envelope_start.expect("the system-only rewrite must produce an envelope patch");
        envelope_end.expect("the system-only rewrite must produce an envelope patch");
        envelope_bytes.expect("the system-only rewrite must produce an envelope patch");

        let (msg_patch_start,): (Option<i32>,) = sqlx::query_as(
            "SELECT sent_patch_start
               FROM requests.deltas
              WHERE request_id = $1 AND change_kind_id = 1",
        )
        .bind(97_004_i64)
        .fetch_one(&handle.pool)
        .await?;
        msg_patch_start.expect("the message-content append must produce a per-message patch");

        let reconstructed = crate::store::persistence::rehydrate::fetch_request_sent_body_by_id(
            &handle.pool,
            97_004,
        )
        .await?
        .expect("reconstruction must return Some for a request with request_body");
        assert_eq!(
            reconstructed.as_ref(),
            sent_bytes.as_ref(),
            "reconstruction must fold the envelope AND per-message patches to reproduce \
             the exact bytes forwarded upstream"
        );

        let expected_sha = hex::encode(sha2::Sha256::digest(sent_bytes.as_ref()));
        let actual_sha = hex::encode(sha2::Sha256::digest(reconstructed.as_ref()));
        assert_eq!(
            actual_sha, expected_sha,
            "reconstructed bytes must hash to the true sent sha256 (manifest self-check)"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn insert_request_initial_captures_the_request_envelope() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let wire = br#"{"model":"claude-opus-5","messages":[{"role":"user","content":"hello"}],"system":[{"type":"text","text":"be helpful"}],"tools":[{"name":"Bash","input_schema":{"type":"object"}}],"metadata":{"user_id":"u-1"},"max_tokens":64000,"context_management":{"edits":[]},"output_config":{"effort":"xhigh"},"diagnostics":{"previous_message_id":"msg_01"},"stream":true}"#;

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(99_701)),
            &session_id,
        );
        req.request_body = Some(bytes::Bytes::from_static(wire));
        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;

        let stored: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT request_envelope FROM requests.requests WHERE id = $1")
                .bind(99_701_i64)
                .fetch_one(&handle.pool)
                .await?;
        let envelope =
            stored.ok_or_else(|| anyhow::anyhow!("a JSON-object body must capture an envelope"))?;
        let text = String::from_utf8(envelope)?;

        for fragment in [
            r#""metadata":{"user_id":"u-1"}"#,
            r#""context_management":{"edits":[]}"#,
            r#""output_config":{"effort":"xhigh"}"#,
            r#""diagnostics":{"previous_message_id":"msg_01"}"#,
            r#""stream":true"#,
            r#""system":[{"type":"text","text":"be helpful"}]"#,
        ] {
            assert!(
                text.contains(fragment),
                "the envelope must carry {fragment} verbatim, got: {text}"
            );
        }
        assert!(
            text.contains(r#""messages":[]"#) && text.contains(r#""tools":[]"#),
            "the two rebuildable arrays must be the only thing removed, got: {text}"
        );

        let mut opaque = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(99_702)),
            &session_id,
        );
        opaque.request_body = Some(bytes::Bytes::from_static(b"not json at all"));
        handle
            .insert_request_initial(&opaque, false, IdMode::Ensure)
            .await?;
        let opaque_envelope: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT request_envelope FROM requests.requests WHERE id = $1")
                .bind(99_702_i64)
                .fetch_one(&handle.pool)
                .await?;
        assert!(
            opaque_envelope.is_none(),
            "an unparseable body must leave the envelope NULL rather than capture a partial one"
        );

        db.cleanup().await?;
        Ok(())
    }
}
