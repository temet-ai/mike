#[cfg(test)]
pub mod tests {
    use super::super::PersistHandle;
    use crate::store::models::{InterceptedRequest, RequestStatus};
    use crate::store::persistence::request_write::IdMode;
    use crate::store::persistence::test_support as test_util;

    #[derive(sqlx::FromRow)]
    struct SessionSummaryAssertRow {
        project_id: i64,
        last_status: Option<String>,
        last_seen: chrono::DateTime<chrono::Utc>,
        first_seen: chrono::DateTime<chrono::Utc>,
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
    async fn snapshot_populates_session_summary_on_first_request() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;

        let session_id = uuid::Uuid::new_v4().to_string();
        snapshot_via_handle(&handle, &session_id, 10_001, "a", |req| {
            req.path = "/v1/messages".to_string();
            req.model = Some("claude-sonnet-4-6".to_string());
        })
        .await?;

        let session_uuid = uuid::Uuid::parse_str(&session_id)?;
        let assignment = handle
            .assign_session_project(session_uuid, "foo-project")
            .await
            .map_err(|e| anyhow::anyhow!("assign_session_project failed: {e:?}"))?;

        let (project_id, project_path): (i64, String) = sqlx::query_as(
            "SELECT id, project_path FROM projects.projects WHERE project_path = $1",
        )
        .bind("foo-project")
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(project_path, "foo-project");
        assert_eq!(project_id, assignment.project_id);

        let row: SessionSummaryAssertRow = sqlx::query_as(
            "SELECT project_id, last_status, last_seen, first_seen
             FROM sessions.sessions WHERE session_uuid = $1::text::uuid",
        )
        .bind(&session_id)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(
            row.project_id, project_id,
            "session project_id matches resolved project"
        );
        assert_eq!(row.last_status.as_deref(), Some("Complete"));
        assert_eq!(
            row.first_seen, row.last_seen,
            "first==last on the first snapshot"
        );
        let (request_rows,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::bigint FROM requests.requests r
              JOIN sessions.sessions s ON s.id = r.session_id
              WHERE s.session_uuid = $1::uuid",
        )
        .bind(&session_id)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(
            request_rows, 1,
            "exactly one request row after first snapshot"
        );

        let (req_project_id,): (i64,) = sqlx::query_as(
            "SELECT r.project_id FROM requests.requests r
              JOIN sessions.sessions s ON s.id = r.session_id
              WHERE s.session_uuid = $1::uuid",
        )
        .bind(&session_id)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(req_project_id, project_id);

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn snapshot_persists_n_rows_and_tracks_max_last_seen() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let base =
            chrono::SubsecRound::trunc_subsecs(chrono::Utc::now() + chrono::Duration::hours(1), 6);
        let mut max_created = base;
        for i in 0..10u64 {
            let created = base + chrono::Duration::milliseconds(i as i64 * 17);
            max_created = created;
            snapshot_via_handle(&handle, &session_id, 20_000 + i, "n", |req| {
                req.project = Some("multi-snap".to_string());
                req.created_at = created;
                req.completed_at = Some(created);
                req.model = Some(if i % 2 == 0 {
                    "claude-sonnet-4-6".to_string()
                } else {
                    "claude-haiku-4-5".to_string()
                });
                req.path = if i == 9 {
                    "/v1/messages/last".to_string()
                } else {
                    "/v1/messages".to_string()
                };
            })
            .await?;
        }

        let row: SessionSummaryAssertRow = sqlx::query_as(
            "SELECT project_id, last_status, last_seen, first_seen
             FROM sessions.sessions WHERE session_uuid = $1::text::uuid",
        )
        .bind(&session_id)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(row.last_status.as_deref(), Some("Complete"));
        assert_eq!(row.last_seen, max_created);

        let (rows,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::bigint FROM requests.requests r
              JOIN sessions.sessions s ON s.id = r.session_id
              WHERE s.session_uuid = $1::uuid",
        )
        .bind(&session_id)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(rows, 10, "exactly 10 request rows must be persisted");

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn snapshot_idempotent_retry_does_not_double_count() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let pinned_time = chrono::Utc::now();
        let make_req = || {
            let mut req = crate::store::tests::make_test_request(
                Some(crate::store::models::RequestId::from_raw(30_001)),
                &session_id,
            );
            req.status = RequestStatus::Complete;
            req.created_at = pinned_time;
            req.completed_at = Some(pinned_time);
            req.project = Some("idempotent-proj".to_string());
            req.model = Some("claude-sonnet-4-6".to_string());
            req.path = "/v1/messages".to_string();
            req
        };

        let req1 = make_req();
        handle
            .insert_request_initial(&req1, true, IdMode::Ensure)
            .await?;
        handle.snapshot_request(&req1).await?;
        let req2 = make_req();
        handle
            .insert_request_initial(&req2, false, IdMode::Ensure)
            .await?;
        handle.snapshot_request(&req2).await?;

        let (rows,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::bigint FROM requests.requests r
              JOIN sessions.sessions s ON s.id = r.session_id
              WHERE s.session_uuid = $1::uuid",
        )
        .bind(&session_id)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(
            rows, 1,
            "idempotent retry on same (id, created_at) must not insert a duplicate row"
        );

        db.cleanup().await?;
        Ok(())
    }
}
