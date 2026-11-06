#[cfg(test)]
mod tests {
    use super::super::PersistHandle;
    use crate::store::persistence::request_write::IdMode;
    use crate::store::persistence::test_support as test_util;

    #[tokio::test]
    async fn insert_request_initial_persists_operator_author_kind() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(99_601)),
            &session_id,
        );
        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;

        let code: String = sqlx::query_scalar(
            "SELECT ak.code FROM requests.requests r
               JOIN requests.author_kinds ak ON ak.id = r.author_kind_id
              WHERE r.id = $1",
        )
        .bind(99_601_i64)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(
            code, "operator",
            "ordinary human text on the primary lane must classify as operator, not the unknown default"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn insert_request_initial_persists_agent_author_kind_for_subagent_turn()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(99_602)),
            &session_id,
        );
        req.turn_origin = "subagent".to_string();
        req.subagent_persona = Some("code-reviewer".to_string());
        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;

        let code: String = sqlx::query_scalar(
            "SELECT ak.code FROM requests.requests r
               JOIN requests.author_kinds ak ON ak.id = r.author_kind_id
              WHERE r.id = $1",
        )
        .bind(99_602_i64)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(
            code, "agent",
            "a subagent turn_origin must classify as agent, not the unknown default"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn insert_request_initial_persists_harness_author_kind_for_probe_turn()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut req = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(99_603)),
            &session_id,
        );
        req.turn_origin = "quota_probe".to_string();
        handle
            .insert_request_initial(&req, true, IdMode::Ensure)
            .await?;

        let code: String = sqlx::query_scalar(
            "SELECT ak.code FROM requests.requests r
               JOIN requests.author_kinds ak ON ak.id = r.author_kind_id
              WHERE r.id = $1",
        )
        .bind(99_603_i64)
        .fetch_one(&handle.pool)
        .await?;
        assert_eq!(
            code, "harness",
            "a probe turn_origin must classify as harness, not the unknown default"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn insert_request_initial_persists_unknown_author_kind_for_empty_prompt()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut empty = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(99_604)),
            &session_id,
        );
        empty.human_prompt = String::new();
        empty.last_user_message = String::new();
        empty.is_tool_loop = false;
        handle
            .insert_request_initial(&empty, true, IdMode::Ensure)
            .await?;

        let sibling = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(99_605)),
            &session_id,
        );
        handle
            .insert_request_initial(&sibling, false, IdMode::Ensure)
            .await?;

        let codes: Vec<(i64, String)> = sqlx::query_as(
            "SELECT r.id, ak.code FROM requests.requests r
               JOIN requests.author_kinds ak ON ak.id = r.author_kind_id
              WHERE r.id IN ($1, $2)",
        )
        .bind(99_604_i64)
        .bind(99_605_i64)
        .fetch_all(&handle.pool)
        .await?;
        let empty_code = codes
            .iter()
            .find(|(id, _)| *id == 99_604)
            .map(|(_, code)| code.as_str());
        let sibling_code = codes
            .iter()
            .find(|(id, _)| *id == 99_605)
            .map(|(_, code)| code.as_str());
        assert_eq!(
            empty_code,
            Some("unknown"),
            "a primary-lane row with no attributable text on either side must classify as unknown"
        );
        assert_eq!(
            sibling_code,
            Some("operator"),
            "the sibling non-empty-prompt row must classify differently, proving the column responds to input rather than sitting at a static default"
        );

        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn insert_request_initial_persists_harness_author_kind_for_stripped_envelope_turns()
    -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let mut envelope = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(99_606)),
            &session_id,
        );
        envelope.human_prompt = String::new();
        envelope.last_user_message = "<system-reminder>\n[SYSTEM NOTIFICATION - NOT USER INPUT]\n<task-notification>\n<status>completed</status>\n</task-notification>\n</system-reminder>".to_string();
        envelope.is_tool_loop = false;
        handle
            .insert_request_initial(&envelope, true, IdMode::Ensure)
            .await?;

        let mut tool_loop = crate::store::tests::make_test_request(
            Some(crate::store::models::RequestId::from_raw(99_607)),
            &session_id,
        );
        tool_loop.human_prompt = String::new();
        tool_loop.last_user_message = String::new();
        tool_loop.is_tool_loop = true;
        handle
            .insert_request_initial(&tool_loop, false, IdMode::Ensure)
            .await?;

        let codes: Vec<(i64, String)> = sqlx::query_as(
            "SELECT r.id, ak.code FROM requests.requests r
               JOIN requests.author_kinds ak ON ak.id = r.author_kind_id
              WHERE r.id IN ($1, $2)",
        )
        .bind(99_606_i64)
        .bind(99_607_i64)
        .fetch_all(&handle.pool)
        .await?;
        let code_for = |id: i64| {
            codes
                .iter()
                .find(|(row_id, _)| *row_id == id)
                .map(|(_, code)| code.as_str())
        };
        assert_eq!(
            code_for(99_606),
            Some("harness"),
            "an all-envelope turn must persist harness, not unknown — the markers naming its author are the exact bytes extract_human_prompt removes"
        );
        assert_eq!(
            code_for(99_607),
            Some("harness"),
            "a tool-result continuation must persist harness: the harness ran the tool and fed the result back"
        );

        db.cleanup().await?;
        Ok(())
    }
}
