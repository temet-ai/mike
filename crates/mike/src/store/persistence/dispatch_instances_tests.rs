#[cfg(test)]
mod tests {

    use super::super::PersistHandle;
    use crate::store::persistence::request_write::IdMode;
    use crate::store::persistence::test_support as test_util;

    fn sample_system_blocks(role_text: &str) -> serde_json::Value {
        serde_json::json!([
            {"type": "text", "text": "cc_version=1.2.3; cch=abc"},
            {"type": "text", "text": "identity preamble"},
            {"type": "text", "text": role_text},
        ])
    }

    #[tokio::test]
    async fn subagent_binds_to_an_instance_path_one_never_bound() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();
        let persona = "Rust Development Expert";

        let mut orchestrator = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(99_100)),
            &session_id,
        );
        orchestrator.turn_origin = "primary_first".to_string();
        orchestrator.tool_calls = vec![crate::types::ToolCall {
            tool_id: "tu-135-001".to_string(),
            name: "Agent".to_string(),
            input_json: serde_json::json!({"subagent_type": "rust-developer"}).to_string(),
            complete: true,
        }];
        handle
            .insert_request_initial(&orchestrator, true, IdMode::Ensure)
            .await?;
        handle.create_instances_for_dispatches(&session_id).await?;

        let session_db_id: i64 = sqlx::query_scalar(
            "SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid",
        )
        .bind(&session_id)
        .fetch_one(handle.pool())
        .await?;

        let instance_id: i64 = sqlx::query_scalar(
            "UPDATE agents.instances
                SET subagent_persona = $2, first_request_id = NULL, started_at = NULL
              WHERE session_id = $1
              RETURNING id",
        )
        .bind(session_db_id)
        .bind(persona)
        .fetch_one(handle.pool())
        .await?;

        let mut subagent = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(99_101)),
            &session_id,
        );
        subagent.turn_origin = "subagent".to_string();
        subagent.subagent_persona = Some(persona.to_string());
        handle
            .insert_request_initial(&subagent, false, IdMode::Ensure)
            .await?;

        let bound: Option<i64> = sqlx::query_scalar(
            "SELECT agent_instance_id FROM requests.requests
              WHERE session_id = $1 ORDER BY id DESC LIMIT 1",
        )
        .bind(session_db_id)
        .fetch_one(handle.pool())
        .await?;
        assert_eq!(
            bound,
            Some(instance_id),
            "an unbound instance must still attribute its sub-agent's requests"
        );

        let first: Option<i64> =
            sqlx::query_scalar("SELECT first_request_id FROM agents.instances WHERE id = $1")
                .bind(instance_id)
                .fetch_one(handle.pool())
                .await?;
        assert!(
            first.is_some(),
            "binding must adopt the request as the instance's first"
        );

        db.cleanup().await?;
        Ok(())
    }
    #[tokio::test]
    async fn blank_dispatch_persona_is_stored_as_null() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut orchestrator = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(99_201)),
            &session_id,
        );
        orchestrator.turn_origin = "primary_first".to_string();
        orchestrator.tool_calls = vec![crate::types::ToolCall {
            tool_id: "tu-blank-001".to_string(),
            name: "Agent".to_string(),
            input_json: serde_json::json!({"subagent_type": "code-reviewer"}).to_string(),
            complete: true,
        }];
        handle
            .insert_request_initial(&orchestrator, true, IdMode::Ensure)
            .await?;

        let mut subagent = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(99_202)),
            &session_id,
        );
        subagent.turn_origin = "subagent".to_string();
        subagent.subagent_persona = Some("   ".to_string());
        handle
            .insert_request_initial(&subagent, false, IdMode::Ensure)
            .await?;
        handle.create_instances_for_dispatches(&session_id).await?;

        let blank_personas: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM agents.instances ai
               JOIN sessions.sessions s ON s.id = ai.session_id
              WHERE s.session_uuid = $1::text::uuid
                AND ai.subagent_persona IS NOT NULL
                AND btrim(ai.subagent_persona) = ''",
        )
        .bind(&session_id)
        .fetch_one(handle.pool())
        .await?;
        assert_eq!(
            blank_personas, 0,
            "a blank persona must be stored as NULL, never as an unmatched empty string"
        );

        db.cleanup().await?;
        Ok(())
    }
    #[tokio::test]
    async fn create_instances_for_dispatches_stamps_session_prompt_ledger_row() -> anyhow::Result<()>
    {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut orchestrator = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(98_001)),
            &session_id,
        );
        orchestrator.turn_origin = "primary_first".to_string();
        orchestrator.tool_calls = vec![crate::types::ToolCall {
            tool_id: "tu-stamp-001".to_string(),
            name: "Agent".to_string(),
            input_json: serde_json::json!({"subagent_type": "code-reviewer"}).to_string(),
            complete: true,
        }];
        handle
            .insert_request_initial(&orchestrator, true, IdMode::Ensure)
            .await?;

        let mut subagent = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(98_002)),
            &session_id,
        );
        subagent.turn_origin = "subagent".to_string();
        subagent.message_count = 1;
        subagent.subagent_persona = Some("code-reviewer".to_string());
        subagent.system_prompt = crate::store::models::SystemPromptObservation {
            blocks: Some(sample_system_blocks("code-reviewer role body")),
            ..Default::default()
        };
        handle
            .insert_request_initial(&subagent, false, IdMode::Ensure)
            .await?;

        let before: (Option<i64>, Option<i64>) = sqlx::query_as(
            "SELECT instance_id, agent_id
               FROM system_prompts.session_prompts WHERE request_id = $1",
        )
        .bind(98_002_i64)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(before, (None, None), "ledger row must start unstamped");

        handle.create_instances_for_dispatches(&session_id).await?;

        let (instance_id, agent_id): (Option<i64>, Option<i64>) = sqlx::query_as(
            "SELECT instance_id, agent_id
               FROM system_prompts.session_prompts WHERE request_id = $1",
        )
        .bind(98_002_i64)
        .fetch_one(&handle.pool)
        .await?;
        assert!(
            instance_id.is_some(),
            "instance_id must be stamped after the dispatch resolves"
        );
        assert!(agent_id.is_some(), "agent_id must be stamped alongside it");

        let agent_id_bound =
            agent_id.ok_or_else(|| anyhow::anyhow!("agent_id must be Some (asserted above)"))?;
        let agent_name: String = sqlx::query_scalar("SELECT name FROM agents.agents WHERE id = $1")
            .bind(agent_id_bound)
            .fetch_one(&handle.pool)
            .await?;
        assert_eq!(agent_name, "code-reviewer");

        db.cleanup().await?;
        Ok(())
    }
    #[tokio::test]
    async fn insert_request_initial_records_dispatch_when_subagent_type_omitted()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(98_101)),
            &session_id,
        );
        req.tool_calls = vec![crate::types::ToolCall {
            tool_id: "tu-no-subagent-type-001".to_string(),
            name: "Agent".to_string(),
            input_json: serde_json::json!({"description": "no type given"}).to_string(),
            complete: true,
        }];
        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;

        let agent_name: Option<String> = sqlx::query_scalar(
            "SELECT ag.name
               FROM agents.dispatches ad
               JOIN tools.uses tu ON tu.id = ad.tool_use_id
               JOIN agents.agents ag ON ag.id = ad.agent_id
              WHERE tu.wire_tool_use_id = $1",
        )
        .bind("tu-no-subagent-type-001")
        .fetch_optional(&handle.pool)
        .await?;
        assert_eq!(
            agent_name.as_deref(),
            Some("general-purpose"),
            "Agent call with no subagent_type must still record a dispatch, defaulted to general-purpose"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn agent_tool_result_backfills_worker_producer_id() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut orchestrator = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(97_001)),
            &session_id,
        );
        orchestrator.turn_origin = "primary_first".to_string();
        orchestrator.tool_calls = vec![crate::types::ToolCall {
            tool_id: "tu-w3-001".to_string(),
            name: "Agent".to_string(),
            input_json: serde_json::json!({"subagent_type": "rust-developer"}).to_string(),
            complete: true,
        }];
        handle
            .insert_request_initial(&orchestrator, true, IdMode::Ensure)
            .await?;

        let producer_id = "a53fd635aaceeed5c";
        let content = serde_json::json!([{
            "type": "text",
            "text": format!(
                "Async agent launched successfully.\nagentId: {producer_id} (internal ID - do not mention to user.)\nThe agent is working in the background."
            )
        }]);
        let bytes = serde_json::to_vec(&content)?;
        let sha = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(&bytes);
            hex::encode(h.finalize())
        };
        let mut follow_up = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(97_002)),
            &session_id,
        );
        follow_up.tool_results_in_body = vec![crate::store::models::ToolResult {
            tool_use_id: "tu-w3-001".to_string(),
            is_error: false,
            content_sha256: sha,
            content_bytes: bytes.len() as u64,
            content,
        }];
        handle
            .insert_request_initial(&follow_up, false, IdMode::Ensure)
            .await?;

        let worker_producer_id_query = "SELECT d.worker_producer_id
               FROM agents.dispatches d
               JOIN tools.uses u ON u.id = d.tool_use_id
              WHERE u.wire_tool_use_id = $1";
        let stored: Option<String> = sqlx::query_scalar(worker_producer_id_query)
            .bind("tu-w3-001")
            .fetch_one(&handle.pool)
            .await?;
        assert_eq!(
            stored.as_deref(),
            Some(producer_id),
            "captured Agent result must backfill worker_producer_id"
        );

        handle
            .insert_request_initial(&follow_up, false, IdMode::Ensure)
            .await?;
        let stored_again: Option<String> = sqlx::query_scalar(worker_producer_id_query)
            .bind("tu-w3-001")
            .fetch_one(&handle.pool)
            .await?;
        assert_eq!(
            stored_again.as_deref(),
            Some(producer_id),
            "re-running the extractor against the same result must change nothing"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn tool_result_without_agent_id_line_leaves_worker_producer_id_null() -> anyhow::Result<()>
    {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut orchestrator = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(97_101)),
            &session_id,
        );
        orchestrator.turn_origin = "primary_first".to_string();
        orchestrator.tool_calls = vec![crate::types::ToolCall {
            tool_id: "tu-w3-101".to_string(),
            name: "Agent".to_string(),
            input_json: serde_json::json!({"subagent_type": "rust-developer"}).to_string(),
            complete: true,
        }];
        handle
            .insert_request_initial(&orchestrator, true, IdMode::Ensure)
            .await?;

        let content = serde_json::json!([{"type": "text", "text": "still running, no id yet"}]);
        let bytes = serde_json::to_vec(&content)?;
        let sha = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(&bytes);
            hex::encode(h.finalize())
        };
        let mut follow_up = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(97_102)),
            &session_id,
        );
        follow_up.tool_results_in_body = vec![crate::store::models::ToolResult {
            tool_use_id: "tu-w3-101".to_string(),
            is_error: false,
            content_sha256: sha,
            content_bytes: bytes.len() as u64,
            content,
        }];
        handle
            .insert_request_initial(&follow_up, false, IdMode::Ensure)
            .await?;

        let stored: Option<String> = sqlx::query_scalar(
            "SELECT d.worker_producer_id
               FROM agents.dispatches d
               JOIN tools.uses u ON u.id = d.tool_use_id
              WHERE u.wire_tool_use_id = $1",
        )
        .bind("tu-w3-101")
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(stored, None);

        db.cleanup().await?;
        Ok(())
    }
}
