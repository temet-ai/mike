#[cfg(test)]
pub mod tests {
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
    async fn insert_request_initial_captures_main_lane_session_prompt() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut first = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(96_001)),
            &session_id,
        );
        first.turn_origin = "primary_first".to_string();
        let first_blocks = sample_system_blocks("first role body");
        first.system_prompt = crate::store::models::SystemPromptObservation {
            blocks: Some(first_blocks.clone()),
            ..Default::default()
        };
        handle
            .insert_request_initial(&first, true, IdMode::Ensure)
            .await?;

        let mut second = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(96_002)),
            &session_id,
        );
        second.turn_origin = "primary".to_string();
        second.system_prompt = crate::store::models::SystemPromptObservation {
            blocks: Some(sample_system_blocks(
                "second role body — must not overwrite",
            )),
            ..Default::default()
        };
        handle
            .insert_request_initial(&second, false, IdMode::Ensure)
            .await?;

        #[derive(sqlx::FromRow)]
        struct MainRow {
            request_id: i64,
            blocks: serde_json::Value,
            content_sha256: Option<String>,
        }
        let rows: Vec<MainRow> = sqlx::query_as(
            "SELECT sp.request_id, sp.blocks, sp.content_sha256
               FROM system_prompts.session_prompts sp
               JOIN sessions.sessions s ON s.id = sp.session_id
              WHERE s.session_uuid = $1::text::uuid AND sp.lane = 'main'",
        )
        .bind(&session_id)
        .fetch_all(&handle.pool)
        .await?;

        assert_eq!(
            rows.len(),
            1,
            "exactly one main-lane row must exist regardless of how many primary/primary_first requests follow"
        );
        assert_eq!(
            rows[0].request_id, 96_001,
            "the main row must belong to the FIRST main-lane request, not a later continuation"
        );
        assert_eq!(
            rows[0].blocks, first_blocks,
            "the main row's blocks must be the first request's array, untouched by the second"
        );
        assert!(
            rows[0]
                .content_sha256
                .as_deref()
                .is_some_and(|s| s.len() == 64),
            "content_sha256 must be a 64-char hex marker"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn insert_request_initial_captures_subagent_lane_with_persona() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(97_001)),
            &session_id,
        );
        req.turn_origin = "subagent".to_string();
        req.message_count = 1;
        req.subagent_persona = Some("code-reviewer".to_string());
        req.system_prompt = crate::store::models::SystemPromptObservation {
            blocks: Some(sample_system_blocks("subagent role body")),
            ..Default::default()
        };
        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;

        #[derive(sqlx::FromRow)]
        struct SubagentRow {
            lane: String,
            subagent_persona: Option<String>,
            instance_id: Option<i64>,
            agent_id: Option<i64>,
        }
        let row: SubagentRow = sqlx::query_as(
            "SELECT sp.lane, sp.subagent_persona, sp.instance_id, sp.agent_id
               FROM system_prompts.session_prompts sp
               JOIN requests.requests r ON r.id = sp.request_id
              WHERE r.id = $1",
        )
        .bind(97_001_i64)
        .fetch_one(&handle.pool)
        .await?;

        assert_eq!(row.lane, "subagent");
        assert_eq!(row.subagent_persona.as_deref(), Some("code-reviewer"));
        assert_eq!(
            row.instance_id, None,
            "instance_id must stay NULL until create_instances_for_dispatches stamps it"
        );
        assert_eq!(row.agent_id, None);

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn insert_request_initial_subagent_persona_never_mints_agent_row() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();
        let persona = "a wholly novel persona sentence never seen as a subagent_type";

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(97_301)),
            &session_id,
        );
        req.turn_origin = "subagent".to_string();
        req.message_count = 1;
        req.subagent_persona = Some(persona.to_string());
        req.system_prompt = crate::store::models::SystemPromptObservation {
            blocks: Some(sample_system_blocks("novel persona role body")),
            ..Default::default()
        };
        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agents.agents WHERE name = $1")
            .bind(persona)
            .fetch_one(&handle.pool)
            .await?;
        assert_eq!(
            count, 0,
            "a subagent persona must never mint its own agents.agents row"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn insert_request_initial_captures_workflow_subagent_lane() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(97_101)),
            &session_id,
        );
        req.turn_origin = "workflow_subagent".to_string();
        req.message_count = 1;
        req.subagent_persona = Some("workflow-agent".to_string());
        req.system_prompt = crate::store::models::SystemPromptObservation {
            blocks: Some(sample_system_blocks("workflow subagent role body")),
            ..Default::default()
        };
        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;

        let lane: String = sqlx::query_scalar(
            "SELECT lane FROM system_prompts.session_prompts WHERE request_id = $1",
        )
        .bind(97_101_i64)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(lane, "subagent");

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn insert_request_initial_skips_subagent_capture_when_not_conversation_start()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(97_201)),
            &session_id,
        );
        req.turn_origin = "subagent".to_string();
        req.message_count = 3;
        req.subagent_persona = Some("code-reviewer".to_string());
        req.system_prompt = crate::store::models::SystemPromptObservation {
            blocks: Some(sample_system_blocks("continuation role body")),
            ..Default::default()
        };
        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM system_prompts.session_prompts WHERE request_id = $1",
        )
        .bind(97_201_i64)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(
            count, 0,
            "message_count != 1 subagent requests must not capture a ledger row"
        );

        db.cleanup().await?;
        Ok(())
    }
}
