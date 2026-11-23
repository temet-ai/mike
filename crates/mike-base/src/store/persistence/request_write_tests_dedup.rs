#[cfg(test)]
mod tests {
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
