#[cfg(test)]
mod tests {
    use super::super::PersistHandle;
    use crate::store::models::{InterceptedRequest, RequestId, ThreadKind, ThreadLink};
    use crate::store::persistence::request_write::IdMode;
    use crate::store::persistence::test_support as test_util;

    fn request(
        id: u64,
        session_id: &str,
        second: i64,
        messages: serde_json::Value,
    ) -> InterceptedRequest {
        let mut req =
            crate::store::tests::make_test_request(Some(RequestId::from_raw(id)), session_id);
        req.created_at =
            chrono::DateTime::from_timestamp(1_790_000_000 + second, 0).unwrap_or_default();
        req.request_body = Some(bytes::Bytes::from(
            serde_json::json!({ "messages": messages }).to_string(),
        ));
        req
    }

    fn link(kind: ThreadKind, predecessor: Option<u64>) -> Option<ThreadLink> {
        Some(ThreadLink {
            kind,
            previous_message_id: Some("msg_not_captured".to_string()),
            predecessor: predecessor.map(RequestId::from_raw),
            predecessor_instance: None,
        })
    }

    fn msg(role: &str, text: &str) -> serde_json::Value {
        serde_json::json!({"role": role, "content": text})
    }

    async fn stored_messages(handle: &PersistHandle) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM messages.messages")
            .fetch_one(&handle.pool)
            .await?)
    }

    async fn deltas(handle: &PersistHandle, request_id: u64) -> anyhow::Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT ck.code FROM requests.deltas d
               JOIN requests.delta_change_kinds ck ON ck.id = d.change_kind_id
              WHERE d.request_id = $1 ORDER BY ck.code, d.ord",
        )
        .bind(i64::try_from(request_id)?)
        .fetch_all(&handle.pool)
        .await?)
    }

    async fn rule(handle: &PersistHandle, request_id: u64) -> anyhow::Result<String> {
        Ok(sqlx::query_scalar(
            "SELECT pr.code FROM requests.requests r
               JOIN requests.predecessor_rules pr ON pr.id = r.predecessor_rule_id
              WHERE r.id = $1",
        )
        .bind(i64::try_from(request_id)?)
        .fetch_one(&handle.pool)
        .await?)
    }

    #[tokio::test]
    async fn full_resend_after_continuations_stores_only_its_new_message() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session = uuid::Uuid::new_v4().to_string();

        let first = request(98_101, &session, 0, serde_json::json!([msg("user", "one")]));
        handle
            .insert_request_initial(&first, true, IdMode::Ensure)
            .await?;
        let mut second = request(
            98_102,
            &session,
            1,
            serde_json::json!([msg("assistant", "reply one"), msg("user", "two")]),
        );
        second.thread = link(ThreadKind::Continue, Some(98_101));
        handle
            .insert_request_initial(&second, true, IdMode::Ensure)
            .await?;
        assert_eq!(stored_messages(&handle).await?, 3);

        let resend = request(
            98_103,
            &session,
            2,
            serde_json::json!([
                msg("user", "one"),
                msg("assistant", "reply one"),
                msg("user", "two"),
                msg("assistant", "reply two"),
            ]),
        );
        handle
            .insert_request_initial(&resend, true, IdMode::Ensure)
            .await?;

        assert_eq!(rule(&handle, 98_103).await?, "lane_fallback");
        assert_eq!(
            stored_messages(&handle).await?,
            4,
            "only `reply two` is new; the three earlier messages are reused"
        );
        assert_eq!(deltas(&handle, 98_103).await?, vec!["added".to_string()]);
        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn unresolved_continuation_records_no_removals() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session = uuid::Uuid::new_v4().to_string();

        let first = request(
            98_201,
            &session,
            0,
            serde_json::json!([msg("user", "one"), msg("assistant", "reply one")]),
        );
        handle
            .insert_request_initial(&first, true, IdMode::Ensure)
            .await?;
        let mut next = request(98_202, &session, 1, serde_json::json!([msg("user", "two")]));
        next.thread = link(ThreadKind::Continue, None);
        handle
            .insert_request_initial(&next, true, IdMode::Ensure)
            .await?;

        assert_eq!(rule(&handle, 98_202).await?, "lane_fallback");
        assert_eq!(deltas(&handle, 98_202).await?, vec!["added".to_string()]);
        assert_eq!(stored_messages(&handle).await?, 3);
        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn thread_start_reuses_the_sessions_stored_messages() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session = uuid::Uuid::new_v4().to_string();

        let main = request(
            98_301,
            &session,
            0,
            serde_json::json!([msg("user", "shared opening"), msg("assistant", "reply")]),
        );
        handle
            .insert_request_initial(&main, true, IdMode::Ensure)
            .await?;
        let mut start = request(
            98_302,
            &session,
            1,
            serde_json::json!([msg("user", "shared opening")]),
        );
        start.thread = link(ThreadKind::Create, None);
        handle
            .insert_request_initial(&start, true, IdMode::Ensure)
            .await?;

        assert_eq!(rule(&handle, 98_302).await?, "thread_start");
        assert_eq!(stored_messages(&handle).await?, 2, "the opening is reused");
        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn continuation_repeating_a_turn_note_reuses_the_stored_note() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session = uuid::Uuid::new_v4().to_string();

        let first = request(
            98_501,
            &session,
            0,
            serde_json::json!([msg("user", "one"), msg("system", "style note")]),
        );
        handle
            .insert_request_initial(&first, true, IdMode::Ensure)
            .await?;
        let mut second = request(
            98_502,
            &session,
            1,
            serde_json::json!([msg("assistant", "reply one")]),
        );
        second.thread = link(ThreadKind::Continue, Some(98_501));
        handle
            .insert_request_initial(&second, true, IdMode::Ensure)
            .await?;
        let mut third = request(
            98_503,
            &session,
            2,
            serde_json::json!([msg("user", "two"), msg("system", "style note")]),
        );
        third.thread = link(ThreadKind::Continue, Some(98_502));
        handle
            .insert_request_initial(&third, true, IdMode::Ensure)
            .await?;

        assert_eq!(rule(&handle, 98_503).await?, "thread_link");
        assert_eq!(
            stored_messages(&handle).await?,
            4,
            "the repeated `style note` is stored once"
        );
        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn side_call_reuses_its_own_sessions_messages_only() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let session = uuid::Uuid::new_v4().to_string();
        let other = uuid::Uuid::new_v4().to_string();

        let main = request(98_401, &session, 0, serde_json::json!([msg("user", "ask")]));
        handle
            .insert_request_initial(&main, true, IdMode::Ensure)
            .await?;
        let mut side = request(98_402, &session, 1, serde_json::json!([msg("user", "ask")]));
        side.turn_origin = "title_sidecar".to_string();
        handle
            .insert_request_initial(&side, true, IdMode::Ensure)
            .await?;
        assert_eq!(
            stored_messages(&handle).await?,
            1,
            "the side call reuses `ask`"
        );

        let elsewhere = request(98_403, &other, 2, serde_json::json!([msg("user", "ask")]));
        handle
            .insert_request_initial(&elsewhere, true, IdMode::Ensure)
            .await?;
        assert_eq!(
            stored_messages(&handle).await?,
            2,
            "another session stores its own copy"
        );
        db.cleanup().await?;
        Ok(())
    }
}
