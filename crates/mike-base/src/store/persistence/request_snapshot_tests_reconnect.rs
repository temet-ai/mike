#[cfg(test)]
mod tests {
    use super::super::PersistHandle;
    use crate::store::models::{Diagnostic, DiagnosticCategory, InterceptedRequest, RequestStatus};
    use crate::store::persistence::SENTINEL_PROJECT_PATH;
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
    async fn snapshot_retargets_sentinel_requests_when_project_resolves() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let t1 = chrono::Utc::now();
        snapshot_via_handle(&handle, &session_id, 40_001, "first", |req| {
            req.created_at = t1;
            req.completed_at = Some(t1);
        })
        .await?;
        let t2 = t1 + chrono::Duration::milliseconds(50);
        snapshot_via_handle(&handle, &session_id, 40_002, "second", |req| {
            req.created_at = t2;
            req.completed_at = Some(t2);
        })
        .await?;

        let session_uuid = uuid::Uuid::parse_str(&session_id)?;
        handle
            .assign_session_project(session_uuid, "bar-project")
            .await
            .map_err(|e| anyhow::anyhow!("assign_session_project failed: {e:?}"))?;

        let project_paths: Vec<(String,)> = sqlx::query_as(
            "SELECT project_path FROM projects.projects WHERE project_path IN ($1, $2)
             ORDER BY project_path",
        )
        .bind(SENTINEL_PROJECT_PATH)
        .bind("bar-project")
        .fetch_all(&handle.pool)
        .await?;
        let names: Vec<String> = project_paths.into_iter().map(|(s,)| s).collect();
        assert!(names.contains(&"(no project)".to_string()));
        assert!(names.contains(&"bar-project".to_string()));

        let (sess_project_path,): (String,) = sqlx::query_as(
            "SELECT p.project_path
             FROM sessions.sessions s
             JOIN projects.projects p ON p.id = s.project_id
             WHERE s.session_uuid = $1::text::uuid",
        )
        .bind(&session_id)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(sess_project_path, "bar-project");

        let (sentinel_rows,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::bigint
             FROM requests.requests r
             JOIN projects.projects p ON p.id = r.project_id
             JOIN sessions.sessions s ON s.id = r.session_id
             WHERE s.session_uuid = $1::text::uuid AND p.project_path = $2",
        )
        .bind(&session_id)
        .bind(SENTINEL_PROJECT_PATH)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(
            sentinel_rows, 0,
            "no request row may still point at the sentinel"
        );

        let (real_rows,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::bigint
             FROM requests.requests r
             JOIN projects.projects p ON p.id = r.project_id
             JOIN sessions.sessions s ON s.id = r.session_id
             WHERE s.session_uuid = $1::text::uuid AND p.project_path = $2",
        )
        .bind(&session_id)
        .bind("bar-project")
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(
            real_rows, 2,
            "both seeded requests must now point at the resolved project"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn snapshot_survives_handle_drop_and_reconnect() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let session_id = uuid::Uuid::new_v4().to_string();
        let pinned_time = chrono::Utc::now();
        let title_payload = serde_json::json!({"title": "Roundtrip Title"}).to_string();

        let known_diag = sample_diagnostic("pragmatic shortcut");

        {
            let handle1 = PersistHandle::connect(&db.database_url).await?;

            let mut primary = crate::store::tests::make_test_request(
                Some(crate::store::models::RequestId::from_raw(50_001)),
                &session_id,
            );
            primary.status = RequestStatus::Complete;
            primary.created_at = pinned_time;
            primary.completed_at = Some(pinned_time);
            primary.model = Some("claude-sonnet-4-6".to_string());
            primary.path = "/v1/messages".to_string();
            primary.request_body = Some(bytes::Bytes::from_static(b"req-body-primary"));
            primary.response_body = Some(bytes::Bytes::from_static(b"resp-body-primary"));
            primary.diagnostics = vec![known_diag.clone()];
            handle1
                .insert_request_initial(&primary, true, IdMode::Ensure)
                .await?;
            handle1.snapshot_request(&primary).await?;

            let mut sidecar = crate::store::tests::make_test_request(
                Some(crate::store::models::RequestId::from_raw(50_002)),
                &session_id,
            );
            sidecar.status = RequestStatus::Complete;
            sidecar.created_at = pinned_time + chrono::Duration::milliseconds(20);
            sidecar.completed_at = Some(sidecar.created_at);
            sidecar.model = Some("claude-sonnet-4-6".to_string());
            sidecar.turn_origin = "title_sidecar".to_string();
            sidecar.request_body = Some(bytes::Bytes::from_static(b"req-body-sidecar"));
            let sidecar_body =
                serde_json::json!({"content": [{"type": "text", "text": title_payload}]})
                    .to_string();
            sidecar.response_body = Some(bytes::Bytes::from(sidecar_body));
            handle1
                .insert_request_initial(&sidecar, false, IdMode::Ensure)
                .await?;
            handle1.snapshot_request(&sidecar).await?;

            let session_uuid = uuid::Uuid::parse_str(&session_id)?;
            handle1
                .assign_session_project(session_uuid, "roundtrip-proj")
                .await
                .map_err(|e| anyhow::anyhow!("assign_session_project failed: {e:?}"))?;
        }

        let handle2 = PersistHandle::connect(&db.database_url).await?;

        let session_db_id = handle2
            .resolve_session_db_id(&session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("session not found after reconnect"))?;
        let summary = handle2
            .fetch_session_summary(session_db_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("session summary missing after reconnect"))?;
        assert_eq!(summary.request_count, 2);
        assert_eq!(summary.project_path, "roundtrip-proj");
        assert_eq!(summary.last_status.as_deref(), Some("Complete"));
        assert_eq!(summary.title.as_deref(), Some("Roundtrip Title"));
        assert_eq!(
            summary.models(),
            vec!["claude-sonnet-4-6".to_string()],
            "models list survives the reconnect roundtrip"
        );

        let restored = handle2
            .fetch_request_by_id(50_001)
            .await?
            .ok_or_else(|| anyhow::anyhow!("primary request missing after reconnect"))?;
        assert_eq!(
            restored.diagnostics.len(),
            0,
            "rehydrated requests no longer carry diagnostics in-memory (fetch from request_diagnostics)"
        );

        let diag_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM requests.diagnostics WHERE request_id = $1")
                .bind(50_001_i64)
                .fetch_one(&handle2.pool)
                .await?;
        assert_eq!(
            diag_count, 1,
            "snapshot_request must write diagnostics to request_diagnostics table"
        );

        db.cleanup().await?;
        Ok(())
    }
}
