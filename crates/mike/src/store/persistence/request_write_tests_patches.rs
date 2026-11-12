#[cfg(test)]
mod tests {
    use super::super::PersistHandle;
    use crate::store::persistence::request_write::IdMode;
    use crate::store::persistence::test_support as test_util;

    #[tokio::test]
    async fn insert_request_initial_tools_append_persists_zero_width_patch_byte_exactly()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let cases: [(&[u8], &[u8]); 2] = [
            (
                br#"{"messages":[{"role":"user","content":"hi"}],"tools":[]}"#,
                br#"{"messages":[{"role":"user","content":"hi"}],"tools":[{"name":"mike_session_roster"}]}"#,
            ),
            (
                br#"{"messages":[{"role":"user","content":"hi"}],"tools":[{"name":"Read"}]}"#,
                br#"{"messages":[{"role":"user","content":"hi"}],"tools":[{"name":"Read"},{"name":"mike_session_roster"}]}"#,
            ),
        ];
        let mut request_id = 97_004;
        for (original, sent) in cases {
            let patch = super::super::messages::compute_envelope_patch(original, sent)?
                .ok_or_else(|| anyhow::anyhow!("tools append must produce an envelope patch"))?;
            assert_eq!(patch.start, patch.end);
            for id_mode in [IdMode::Mint, IdMode::Ensure] {
                let session_id = uuid::Uuid::new_v4().to_string();
                let mut req = crate::store::tests::make_test_request(
                    Some(crate::store::models::RequestId::from_raw(request_id)),
                    &session_id,
                );
                request_id += 1;
                req.request_body = Some(bytes::Bytes::copy_from_slice(original));
                req.sent_body = Some(bytes::Bytes::copy_from_slice(sent));
                let outcome = handle.insert_request_initial(&req, true, id_mode).await?;
                let persisted = handle
                    .fetch_request_by_id(outcome.id as u64)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("request must survive the initial INSERT"))?;
                assert_eq!(persisted.request_body.as_deref(), Some(original));
                let reconstructed = handle
                    .fetch_request_sent_body_by_id(outcome.id as u64)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("sent body must reconstruct after insertion"))?;
                assert_eq!(reconstructed.as_ref(), sent);
            }
        }
        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn insert_request_initial_persists_request_body_size() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut with_body = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(97_101)),
            &session_id,
        );
        let body_bytes = bytes::Bytes::from(r#"{"messages":[{"role":"user","content":"hi"}]}"#);
        with_body.request_body = Some(body_bytes.clone());
        let mint_outcome = handle
            .insert_request_initial(&with_body, true, IdMode::Mint)
            .await?;

        let (persisted_size,): (i32,) =
            sqlx::query_as("SELECT request_body_size FROM requests.requests WHERE id = $1")
                .bind(mint_outcome.id)
                .fetch_one(&handle.pool)
                .await?;
        assert_eq!(
            persisted_size as usize,
            body_bytes.len(),
            "persisted size must equal the real body's byte length"
        );

        let no_body = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(97_102)),
            &session_id,
        );
        assert!(
            no_body.request_body.is_none(),
            "fixture default must carry no body for this half of the test"
        );
        handle
            .insert_request_initial(&no_body, false, IdMode::Ensure)
            .await?;

        let (absent_size,): (i32,) =
            sqlx::query_as("SELECT request_body_size FROM requests.requests WHERE id = $1")
                .bind(97_102_i64)
                .fetch_one(&handle.pool)
                .await?;
        assert_eq!(
            absent_size, 0,
            "an absent body must persist as 0, matching byte_size()'s existing semantics"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    #[allow(
        clippy::expect_used,
        reason = "test fixture; a NULL here is the failure the test exists to catch"
    )]
    async fn insert_request_initial_identity_request_leaves_sent_patch_null() -> anyhow::Result<()>
    {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(97_002)),
            &session_id,
        );
        let body = serde_json::json!({
            "messages": [{"role": "user", "content": "unmutated turn"}]
        });
        let body_bytes = bytes::Bytes::from(body.to_string());
        req.request_body = Some(body_bytes.clone());
        req.sent_body = None;
        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;

        let (patch_start,): (Option<i32>,) = sqlx::query_as(
            "SELECT sent_patch_start FROM requests.deltas
              WHERE request_id = $1 AND change_kind_id = 1",
        )
        .bind(97_002_i64)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(
            patch_start, None,
            "an identity request must leave sent_patch_start NULL"
        );

        let reconstructed = crate::store::persistence::rehydrate::fetch_request_sent_body_by_id(
            &handle.pool,
            97_002,
        )
        .await?
        .expect("reconstruction must return Some for a request with request_body");
        assert_eq!(
            reconstructed.as_ref(),
            body_bytes.as_ref(),
            "an identity request's request_sent must equal request_body unchanged"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    #[allow(
        clippy::expect_used,
        reason = "test fixture; a NULL here is the failure the test exists to catch"
    )]
    async fn insert_request_initial_sent_patch_preserves_cache_control_in_raw_reconstruction()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(97_003)),
            &session_id,
        );
        let client_body = serde_json::json!({
            "messages": [{
                "role": "user",
                "content": [{"type": "text", "text": "hi", "cache_control": {"type": "ephemeral"}}]
            }]
        });
        let client_bytes = bytes::Bytes::from(client_body.to_string());
        let sent_body = serde_json::json!({
            "messages": [{
                "role": "user",
                "content": [{"type": "text", "text": "hi INJECTED", "cache_control": {"type": "ephemeral"}}]
            }]
        });
        let sent_bytes = bytes::Bytes::from(sent_body.to_string());

        req.request_body = Some(client_bytes);
        req.sent_body = Some(sent_bytes.clone());
        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;

        let reconstructed = crate::store::persistence::rehydrate::fetch_request_sent_body_by_id(
            &handle.pool,
            97_003,
        )
        .await?
        .expect("reconstruction must return Some for a request with request_body");
        assert_eq!(
            reconstructed.as_ref(),
            sent_bytes.as_ref(),
            "raw reconstruction must preserve cache_control byte-exactly, unaffected by canonical stripping"
        );
        assert!(
            std::str::from_utf8(&reconstructed)?.contains("cache_control"),
            "cache_control must survive reconstruction, not be dropped like it is in the canonical form"
        );

        let canonical_body: (serde_json::Value,) = sqlx::query_as(
            "SELECT m.body
               FROM requests.messages rm
               JOIN messages.messages m ON m.id = rm.message_id
              WHERE rm.request_id = $1 AND rm.ord = 0",
        )
        .bind(97_003_i64)
        .fetch_one(&handle.pool)
        .await?;
        assert!(
            !canonical_body.0.to_string().contains("cache_control"),
            "the canonical dedup form must have cache_control stripped"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn transformation_manifest_some_value_rehydrates_byte_equal() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let manifest = serde_json::json!({
            "schema_version": 1,
            "original_sha256": "a".repeat(64),
            "sent_sha256": "b".repeat(64),
            "stages": [
                {"kind": "filter_rewrite", "reason": "pii_redact", "byte_range": [120, 340]},
                {"kind": "compression", "reason": "large_file_read", "byte_range": [4096, 20480]},
            ],
        });

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(95_201)),
            &session_id,
        );
        req.transformation_manifest = Some(manifest.clone());

        let request_id = handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?
            .id;

        let fetched = crate::store::persistence::rehydrate::fetch_request_by_id(
            &handle.pool,
            request_id as u64,
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("request row must exist after insert_request_initial"))?;

        assert_eq!(
            fetched.transformation_manifest,
            Some(manifest),
            "transformation_manifest must rehydrate byte-equal to what was persisted"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn transformation_manifest_none_rehydrates_none() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(95_202)),
            &session_id,
        );
        req.transformation_manifest = None;

        let request_id = handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?
            .id;

        let fetched = crate::store::persistence::rehydrate::fetch_request_by_id(
            &handle.pool,
            request_id as u64,
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("request row must exist after insert_request_initial"))?;

        assert_eq!(
            fetched.transformation_manifest, None,
            "transformation_manifest must rehydrate as None when never populated"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn insert_request_initial_two_field_mutation_reconstructs_the_sent_body()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let cases: [(&[u8], &[u8]); 3] = [
            (
                br#"{"messages":[{"role":"user","content":"hi"}],"system":"OLD"}"#,
                br#"{"messages":[{"role":"user","content":"hi"}],"system":"NEW","tools":[{"name":"mike_roster"}]}"#,
            ),
            (
                br#"{"messages":[{"role":"user","content":"hi"}],"system":"OLD"}"#,
                br#"{"messages":[{"role":"user","content":"hi RECALL"}],"system":"NEW","tools":[{"name":"mike_roster"}]}"#,
            ),
            (
                br#"{"model":"m","system":"OLD-SYS","messages":[{"role":"user","content":"hi"}],"tail":"OLD"}"#,
                br#"{"model":"m","system":"NEW-SYS","messages":[{"role":"user","content":"hi EXTRA"}],"tail":"NEW"}"#,
            ),
        ];
        let mut request_id = 97_401;
        for (original, sent) in cases {
            let session_id = uuid::Uuid::new_v4().to_string();
            let mut req = crate::store::tests::make_test_request(
                Some(crate::store::models::RequestId::from_raw(request_id)),
                &session_id,
            );
            request_id += 1;
            req.request_body = Some(bytes::Bytes::copy_from_slice(original));
            req.sent_body = Some(bytes::Bytes::copy_from_slice(sent));
            let outcome = handle
                .insert_request_initial(&req, true, IdMode::Mint)
                .await?;
            let reconstructed = handle
                .fetch_request_sent_body_by_id(outcome.id as u64)
                .await?
                .ok_or_else(|| anyhow::anyhow!("sent body must reconstruct after insertion"))?;
            assert_eq!(
                reconstructed.as_ref(),
                sent,
                "request {request_id}: the captured sent body must equal the forwarded bytes"
            );
        }
        db.cleanup().await?;
        Ok(())
    }
}
