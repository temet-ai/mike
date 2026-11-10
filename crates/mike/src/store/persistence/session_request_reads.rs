use super::{PersistError, PersistHandle};
use super::{content_search, reconstruct, rehydrate};
use crate::store::models::InterceptedRequest;

impl PersistHandle {
    pub(crate) async fn fetch_session_summary(
        &self,
        session_db_id: i64,
    ) -> Result<Option<rehydrate::SessionSummaryRow>, PersistError> {
        crate::metrics::time_db(
            "fetch_session_summary",
            rehydrate::fetch_session_summary(&self.pool, session_db_id),
        )
        .await
    }
    pub(crate) async fn fetch_session_prompt_roster(
        &self,
        session_db_id: i64,
    ) -> Result<(Vec<rehydrate::PromptRosterRow>, bool), PersistError> {
        crate::metrics::time_db(
            "fetch_session_prompt_roster",
            rehydrate::fetch_session_prompt_roster(&self.pool, session_db_id),
        )
        .await
    }
    pub(crate) async fn fetch_session_model_breakdown(
        &self,
        session_db_id: i64,
    ) -> Result<Vec<rehydrate::ModelBreakdownRow>, PersistError> {
        crate::metrics::time_db(
            "fetch_session_model_breakdown",
            rehydrate::fetch_session_model_breakdown(&self.pool, session_db_id),
        )
        .await
    }
    pub(crate) async fn fetch_session_token_usage(
        &self,
        session_db_id: i64,
    ) -> Result<Vec<rehydrate::SessionModelTokenUsageRow>, PersistError> {
        crate::metrics::time_db(
            "fetch_session_token_usage",
            rehydrate::fetch_session_token_usage(&self.pool, session_db_id),
        )
        .await
    }
    pub(crate) async fn fetch_request_by_id(
        &self,
        request_id: u64,
    ) -> Result<Option<InterceptedRequest>, PersistError> {
        crate::metrics::time_db(
            "fetch_request_by_id",
            rehydrate::fetch_request_by_id(&self.pool, request_id),
        )
        .await
    }
    pub(crate) async fn fetch_request_body_by_id(
        &self,
        request_id: u64,
    ) -> Result<Option<reconstruct::ServedBody>, PersistError> {
        crate::metrics::time_db(
            "fetch_request_body_by_id",
            rehydrate::fetch_request_body_by_id(&self.pool, request_id),
        )
        .await
    }
    pub(crate) async fn fetch_request_sent_body_by_id(
        &self,
        request_id: u64,
    ) -> Result<Option<bytes::Bytes>, PersistError> {
        crate::metrics::time_db(
            "fetch_request_sent_body_by_id",
            rehydrate::fetch_request_sent_body_by_id(&self.pool, request_id),
        )
        .await
    }
    pub(crate) async fn fetch_response_body_by_id(
        &self,
        request_id: u64,
    ) -> Result<Option<bytes::Bytes>, PersistError> {
        crate::metrics::time_db(
            "fetch_response_body_by_id",
            rehydrate::fetch_response_body_by_id(&self.pool, request_id),
        )
        .await
    }
    pub(crate) async fn fetch_response_bodies_batch(
        &self,
        request_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, bytes::Bytes>, PersistError> {
        crate::metrics::time_db(
            "fetch_response_bodies_batch",
            rehydrate::fetch_response_bodies_batch(&self.pool, request_ids),
        )
        .await
    }
    pub(crate) async fn fetch_request_bodies_batch(
        &self,
        request_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, bytes::Bytes>, PersistError> {
        crate::metrics::time_db(
            "fetch_request_bodies_batch",
            rehydrate::fetch_request_bodies_batch(&self.pool, request_ids),
        )
        .await
    }
    pub(crate) async fn search_content_totals(
        &self,
        query: &str,
        project_id: Option<i64>,
    ) -> Result<content_search::ContentSearchTotals, PersistError> {
        crate::metrics::time_db(
            "search_content_totals",
            content_search::search_content_totals(&self.pool, query, project_id),
        )
        .await
    }
    pub(crate) async fn search_content_page(
        &self,
        query: &str,
        project_id: Option<i64>,
        page: &content_search::PageRequest,
    ) -> Result<Vec<content_search::ContentSearchRow>, PersistError> {
        crate::metrics::time_db(
            "search_content_page",
            content_search::search_content_page(&self.pool, query, project_id, page),
        )
        .await
    }
    pub(crate) async fn fetch_session_request_summaries_paginated(
        &self,
        session_db_id: i64,
        offset: usize,
        limit: usize,
        order: crate::types::SortOrder,
    ) -> Result<
        (
            Vec<crate::store::request_summary::RequestSummaryView>,
            usize,
        ),
        PersistError,
    > {
        crate::metrics::time_db("fetch_session_request_summaries_paginated", async move {
            let (requests, total_seen) = rehydrate::fetch_session_request_summaries_paginated(
                &self.pool,
                session_db_id,
                offset,
                limit,
                order,
            )
            .await?;
            let total = match total_seen {
                Some(n) => n,
                None => rehydrate::count_session_requests(&self.pool, session_db_id).await?,
            };
            Ok((requests, total))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::persistence::request_write::IdMode;
    use crate::store::persistence::test_support as test_util;

    use crate::store::models::RequestStatus;

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

    fn make_test_prompt_sha256() -> String {
        format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        )
    }

    #[tokio::test]
    async fn fetch_session_prompt_roster_returns_all_rows_below_limit() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let base = chrono::Utc::now();
        let blocks = serde_json::json!([{ "type": "text", "text": "below-limit" }]);

        let mut expected_shas: Vec<String> = Vec::with_capacity(5);
        for i in 0..5u64 {
            let sha = make_test_prompt_sha256();
            expected_shas.push(sha.clone());
            let created = base + chrono::Duration::milliseconds(i as i64 * 5);
            let blocks = blocks.clone();
            snapshot_via_handle(&handle, &session_id, 70_000 + i, "rb", move |req| {
                req.created_at = created;
                req.completed_at = Some(created);
                req.system_prompt.prompt_sha256 = Some(sha);
                req.system_prompt.blocks = Some(blocks);
            })
            .await?;
        }

        let session_db_id = handle
            .resolve_session_db_id(&session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("session must exist after snapshots"))?;
        let (rows, truncated) = handle.fetch_session_prompt_roster(session_db_id).await?;
        assert!(
            !truncated,
            "5 distinct shas must NOT trigger the truncation flag"
        );
        assert_eq!(
            rows.len(),
            5,
            "all 5 distinct shas must appear in the roster"
        );
        let returned_shas: Vec<String> = rows.iter().map(|r| r.prompt_sha256.clone()).collect();
        assert_eq!(
            returned_shas, expected_shas,
            "rows must come back in first_request_id ASC order — the order they were seeded"
        );
        for row in &rows {
            assert_eq!(row.occurrence_count, 1, "each sha was seeded exactly once");
            assert!(
                row.composition_id.is_none(),
                "no composition was registered, so composition_id must be NULL"
            );
            assert!(
                row.label.is_none(),
                "no composition was registered, so label must be NULL"
            );
        }

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn fetch_session_prompt_roster_caps_at_limit_and_flags_truncation() -> anyhow::Result<()>
    {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let base = chrono::Utc::now();
        let blocks = serde_json::json!([{ "type": "text", "text": "over-limit" }]);

        let total = rehydrate::PROMPT_ROSTER_LIMIT + 1;
        let mut seeded_shas: Vec<String> = Vec::with_capacity(total);
        for i in 0..total {
            let sha = make_test_prompt_sha256();
            seeded_shas.push(sha.clone());
            let created = base + chrono::Duration::milliseconds(i as i64);
            let blocks = blocks.clone();
            snapshot_via_handle(&handle, &session_id, 80_000 + i as u64, "rc", move |req| {
                req.created_at = created;
                req.completed_at = Some(created);
                req.system_prompt.prompt_sha256 = Some(sha);
                req.system_prompt.blocks = Some(blocks);
            })
            .await?;
        }

        let session_db_id = handle
            .resolve_session_db_id(&session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("session must exist after snapshots"))?;
        let (rows, truncated) = handle.fetch_session_prompt_roster(session_db_id).await?;
        assert!(
            truncated,
            "{total} distinct shas (cap + 1) MUST set the truncation flag"
        );
        assert_eq!(
            rows.len(),
            rehydrate::PROMPT_ROSTER_LIMIT,
            "rows must be capped to PROMPT_ROSTER_LIMIT exactly — never more, never fewer"
        );
        let returned_shas: Vec<String> = rows.iter().map(|r| r.prompt_sha256.clone()).collect();
        assert_eq!(
            returned_shas,
            seeded_shas[..rehydrate::PROMPT_ROSTER_LIMIT].to_vec(),
            "the cap must keep the FIRST PROMPT_ROSTER_LIMIT shas in chronological order"
        );

        db.cleanup().await?;
        Ok(())
    }
}
