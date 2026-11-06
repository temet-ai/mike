#[cfg(test)]
mod tests {
    use super::super::PersistHandle;
    use crate::store::models::{Diagnostic, DiagnosticCategory, InterceptedRequest, RequestStatus};
    use crate::store::persistence::StreamBlockRow;
    use crate::store::persistence::request_write::IdMode;
    use crate::store::persistence::test_support as test_util;

    fn sample_diagnostic(matched_text: &str) -> Diagnostic {
        Diagnostic {
            category: DiagnosticCategory::JustifyingShortcuts,
            analyzer: "test_analyzer".to_string(),
            advice: "stop oversimplifying".to_string(),
            matched_text: matched_text.to_string(),
            match_range: (0, matched_text.len()),
            other_hits: 0,
            created_at: chrono::Utc::now(),
        }
    }

    async fn snapshot_via_handle(
        handle: &PersistHandle,
        session_id: &str,
        memory_id: u64,
        body_marker: &str,
        configure: impl FnOnce(&mut InterceptedRequest),
    ) -> anyhow::Result<()> {
        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(memory_id)),
            session_id,
        );
        req.status = RequestStatus::Complete;
        req.request_body = Some(bytes::Bytes::from(format!("req-body-{body_marker}")));
        req.response_body = Some(bytes::Bytes::from(format!("resp-body-{body_marker}")));
        req.completed_at = Some(req.created_at);
        configure(&mut req);
        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;
        handle.snapshot_request(&req).await?;
        Ok(())
    }

    #[tokio::test]
    async fn snapshot_extracts_title_from_streaming_sidecar_chunks() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();

        let mut sidecar = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(60_001)),
            &session_id,
        );
        sidecar.status = RequestStatus::Complete;
        sidecar.created_at = now;
        sidecar.completed_at = Some(now);
        sidecar.project = Some("stream-title-proj".to_string());
        sidecar.model = Some("claude-sonnet-4-6".to_string());
        sidecar.turn_origin = "title_sidecar".to_string();
        sidecar.is_streaming = true;
        sidecar.response_body = None;

        handle
            .insert_request_initial(&sidecar, true, IdMode::Ensure)
            .await?;

        let request_id = sidecar.id.as_u64() as i64;
        for (seq_ord, chunk) in [r#"{"title":"Some "#, r#"Title"}"#].iter().enumerate() {
            use sha2::Digest as _;
            let mut hasher = sha2::Sha256::new();
            hasher.update(chunk.as_bytes());
            let content_sha256 = hex::encode(hasher.finalize());
            handle
                .insert_stream_block_row(StreamBlockRow {
                    request_id,
                    seq_ord: seq_ord as i32,
                    block_type: "text",
                    content: chunk,
                    signature: None,
                    wire_tool_use_id: None,
                    content_sha256: &content_sha256,
                })
                .await?;
        }

        let accumulated = handle
            .fetch_accumulated_text_for_request(request_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("accumulated text missing for streaming sidecar"))?;
        assert_eq!(
            accumulated, r#"{"title":"Some Title"}"#,
            "chunks must reconstruct the full title JSON before snapshot"
        );

        let (returned_title, _) = handle.snapshot_request(&sidecar).await?;
        assert_eq!(
            returned_title.as_deref(),
            Some("Some Title"),
            "snapshot_request must reconstruct the streaming sidecar title from chunks"
        );

        let session_db_id = handle
            .resolve_session_db_id(&session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("session not found after snapshot"))?;
        let summary = handle
            .fetch_session_summary(session_db_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("session summary missing after snapshot"))?;
        assert_eq!(
            summary.title.as_deref(),
            Some("Some Title"),
            "sessions.title must be set from the streaming sidecar chunk fallback"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn snapshot_writes_request_diagnostics_rows() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let diags = vec![
            sample_diagnostic("first matched span"),
            Diagnostic {
                category: DiagnosticCategory::DeflectingBlame,
                analyzer: "second_analyzer".to_string(),
                advice: "do not deflect".to_string(),
                matched_text: "second matched span (longer with whitespace)".to_string(),
                match_range: (10, 50),
                other_hits: 2,
                created_at: chrono::Utc::now(),
            },
            sample_diagnostic("third matched span"),
        ];
        snapshot_via_handle(&handle, &session_id, 60_001, "rd", |req| {
            req.diagnostics = diags.clone();
        })
        .await?;

        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::bigint
               FROM requests.diagnostics d
               JOIN requests.requests r ON r.id = d.request_id
               JOIN sessions.sessions s ON s.id = r.session_id
              WHERE s.session_uuid = $1::text::uuid",
        )
        .bind(&session_id)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(n, 3, "exactly three request_diagnostics rows expected");

        let matched: Vec<(String,)> = sqlx::query_as(
            "SELECT d.matched_text
               FROM requests.diagnostics d
               JOIN requests.requests r ON r.id = d.request_id
               JOIN sessions.sessions s ON s.id = r.session_id
              WHERE s.session_uuid = $1::text::uuid
              ORDER BY d.matched_text",
        )
        .bind(&session_id)
        .fetch_all(&handle.pool)
        .await?;
        let matched_strings: Vec<String> = matched.into_iter().map(|(s,)| s).collect();
        assert!(matched_strings.contains(&"first matched span".to_string()));
        assert!(
            matched_strings.contains(&"second matched span (longer with whitespace)".to_string())
        );
        assert!(matched_strings.contains(&"third matched span".to_string()));

        sqlx::query!(
            "DELETE FROM requests.requests r
             USING sessions.sessions s
             WHERE r.session_id = s.id AND s.session_uuid = $1::text::uuid",
            &session_id
        )
        .execute(&handle.pool)
        .await?;
        let (after,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::bigint FROM requests.diagnostics rd
             JOIN requests.requests r
               ON r.id = rd.request_id
             JOIN sessions.sessions s
               ON s.id = r.session_id
             WHERE s.session_uuid = $1::text::uuid",
        )
        .bind(&session_id)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(after, 0, "FK cascade must remove children on parent delete");

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn snapshot_operator_turn_user_prompt_uses_human_prompt() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(91_001)),
            &session_id,
        );
        req.status = RequestStatus::Complete;
        req.completed_at = Some(req.created_at);
        req.last_user_message =
            "<system-reminder>\nmetadata\n</system-reminder>\n\nDo the task".to_string();
        req.human_prompt = "Do the task".to_string();
        req.turn_type = "user_prompt".to_string();
        req.turn_origin = "primary".to_string();

        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;
        handle.snapshot_request(&req).await?;

        let (user_prompt,): (String,) = sqlx::query_as(
            "SELECT user_prompt FROM memory.operator_turns
              JOIN sessions.sessions s ON s.id = session_id
             WHERE s.session_uuid = $1::text::uuid",
        )
        .bind(&session_id)
        .fetch_one(&handle.pool)
        .await?;

        assert_eq!(
            user_prompt, "Do the task",
            "user_prompt must be the stripped human_prompt, not the raw last_user_message"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn snapshot_operator_turn_opens_no_turn_when_human_prompt_empty() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(91_002)),
            &session_id,
        );
        req.status = RequestStatus::Complete;
        req.completed_at = Some(req.created_at);
        req.last_user_message = "<system-reminder>\nonly injection\n</system-reminder>".to_string();
        req.human_prompt = String::new();
        req.turn_type = "user_prompt".to_string();
        req.turn_origin = "primary".to_string();

        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;
        handle.snapshot_request(&req).await?;

        let turn_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM memory.operator_turns
              JOIN sessions.sessions s ON s.id = session_id
             WHERE s.session_uuid = $1::text::uuid",
        )
        .bind(&session_id)
        .fetch_one(&handle.pool)
        .await?;

        assert_eq!(
            turn_count, 1,
            "an empty human_prompt opens a machine segment, never an operator turn"
        );
        let opened_by: Option<String> = sqlx::query_scalar(
            "SELECT opened_by FROM memory.operator_turns
              JOIN sessions.sessions s ON s.id = session_id
             WHERE s.session_uuid = $1::text::uuid",
        )
        .bind(&session_id)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(opened_by.as_deref(), Some("machine"));

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn snapshot_operator_turn_skips_harness_and_probe_patterns() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;

        let patterns: &[(&str, &str)] = &[
            (
                "activity-label",
                "Describe your most recent action in 3-5 words",
            ),
            ("hook-feedback", "Stop hook feedback: lint clean"),
            (
                "teammate-message",
                "<teammate-message from=\"x\">hi</teammate-message>",
            ),
            (
                "agent-message",
                "<agent-message from=\"y\">hi</agent-message>",
            ),
            (
                "session-recap",
                "The user stepped away and is coming back. Recap in under 40 words.",
            ),
            (
                "background-event",
                "[SYSTEM NOTIFICATION - NOT USER INPUT]\nThis is an automated background-task event.",
            ),
            (
                "compaction",
                "This session is being continued from a previous conversation that ran out of context.",
            ),
            (
                "compaction-summarize",
                "Summarize this portion of a Claude Code session transcript.",
            ),
            (
                "analysis-probe",
                "Analyze this Claude Code session and extract structured facets.",
            ),
            (
                "analysis-probe-stopping",
                "has the following stopping condition been satisfied",
            ),
        ];

        for (idx, (label, prompt)) in patterns.iter().enumerate() {
            let session_id = uuid::Uuid::new_v4().to_string();
            let mut req = crate::store::tests::make_test_request(
                Some(crate::store::models::RequestId::from_raw(
                    91_100 + idx as u64,
                )),
                &session_id,
            );
            req.status = RequestStatus::Complete;
            req.completed_at = Some(req.created_at);
            req.last_user_message = (*prompt).to_string();
            req.human_prompt = (*prompt).to_string();
            req.turn_type = "user_prompt".to_string();
            req.turn_origin = "primary".to_string();

            handle
                .insert_request_initial(&req, true, IdMode::Ensure)
                .await?;
            handle.snapshot_request(&req).await?;

            let opened_by: Vec<Option<String>> = sqlx::query_scalar(
                "SELECT opened_by FROM memory.operator_turns
                  JOIN sessions.sessions s ON s.id = session_id
                 WHERE s.session_uuid = $1::text::uuid",
            )
            .bind(&session_id)
            .fetch_all(&handle.pool)
            .await?;

            let is_probe = matches!(
                *label,
                "activity-label"
                    | "session-recap"
                    | "compaction-summarize"
                    | "analysis-probe"
                    | "analysis-probe-stopping"
            );
            let expected: Vec<Option<String>> = if is_probe {
                Vec::new()
            } else {
                vec![Some("machine".to_string())]
            };
            assert_eq!(
                opened_by, expected,
                "{label}: probes open no row, in-lane machine text opens a machine segment"
            );
        }

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn snapshot_operator_turn_opens_turn_for_genuine_user_prompt() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(91_200)),
            &session_id,
        );
        req.status = RequestStatus::Complete;
        req.completed_at = Some(req.created_at);
        req.last_user_message = "Fix the flaky retry test".to_string();
        req.human_prompt = "Fix the flaky retry test".to_string();
        req.turn_type = "user_prompt".to_string();
        req.turn_origin = "primary".to_string();

        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;
        handle.snapshot_request(&req).await?;

        let turn_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM memory.operator_turns
              JOIN sessions.sessions s ON s.id = session_id
             WHERE s.session_uuid = $1::text::uuid",
        )
        .bind(&session_id)
        .fetch_one(&handle.pool)
        .await?;

        assert_eq!(
            turn_count, 1,
            "a genuine user prompt must still open exactly one operator_turns row"
        );

        db.cleanup().await?;
        Ok(())
    }
}
