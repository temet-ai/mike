#[cfg(test)]
pub mod tests {
    use super::super::PersistHandle;
    use crate::store::models::{
        InterceptedRequest, RequestId, SystemPromptObservation, SystemPromptSource,
    };
    use crate::store::persistence::request_write::IdMode;
    use crate::store::persistence::test_support as test_util;

    fn request_with_prompt(
        memory_id: u64,
        sha: Option<String>,
        blocks: Option<serde_json::Value>,
    ) -> InterceptedRequest {
        let session_id = uuid::Uuid::new_v4().to_string();
        let mut req = crate::store::tests::make_test_request(
            Some(RequestId::from_raw(memory_id)),
            &session_id,
        );
        req.system_prompt = SystemPromptObservation {
            prompt_sha256: sha,
            blocks,
            composition_id: None,
            composition_applied: false,
            unknown_blocks: None,
            filter_modified: false,
            firing_rule_ids: Vec::new(),
            source: SystemPromptSource::Exact,
        };
        req
    }

    async fn observation(
        handle: &PersistHandle,
        request_id: i64,
    ) -> anyhow::Result<(Option<String>, Option<i64>)> {
        let row: (Option<String>, Option<i64>) = sqlx::query_as(
            "SELECT prompt_sha256, system_prompt_id FROM requests.system_prompts
              WHERE request_id = $1",
        )
        .bind(request_id)
        .fetch_one(&handle.pool)
        .await?;
        Ok(row)
    }

    #[tokio::test]
    async fn insert_request_initial_same_prompt_twice_carries_one_system_prompt_id()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let sha = format!("{:064x}", 0x7a11_u64);
        let blocks = serde_json::json!([{ "type": "text", "text": "carried by id" }]);

        let first = request_with_prompt(96_101, Some(sha.clone()), Some(blocks.clone()));
        let second = request_with_prompt(96_102, Some(sha.clone()), Some(blocks.clone()));
        let first_id = handle
            .insert_request_initial(&first, true, IdMode::Ensure)
            .await?
            .id;
        let second_id = handle
            .insert_request_initial(&second, true, IdMode::Ensure)
            .await?
            .id;
        handle
            .insert_request_initial(&second, true, IdMode::Ensure)
            .await?;

        let (first_sha, first_prompt) = observation(&handle, first_id).await?;
        let (_, second_prompt) = observation(&handle, second_id).await?;
        let first_prompt =
            first_prompt.ok_or_else(|| anyhow::anyhow!("capture must set system_prompt_id"))?;
        assert_eq!(first_sha.as_deref(), Some(sha.as_str()));
        assert_eq!(second_prompt, Some(first_prompt));

        let (registry_rows, stored_blocks): (i64, Option<serde_json::Value>) = sqlx::query_as(
            "SELECT count(*) OVER (), blocks FROM system_prompts.system_prompts
              WHERE prompt_sha256 = $1
              LIMIT 1",
        )
        .bind(&sha)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(registry_rows, 1, "the prompt is registered once");
        assert_eq!(stored_blocks, Some(blocks));

        let observations: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM requests.system_prompts WHERE request_id = $1",
        )
        .bind(second_id)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(
            observations, 1,
            "a re-run does not add a second observation"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn insert_request_initial_without_prompt_leaves_system_prompt_id_null()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;

        let req = request_with_prompt(96_201, None, None);
        let request_id = handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?
            .id;

        assert_eq!(observation(&handle, request_id).await?, (None, None));

        db.cleanup().await?;
        Ok(())
    }
}
