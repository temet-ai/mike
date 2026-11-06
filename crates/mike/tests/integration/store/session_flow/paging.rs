use uuid::Uuid;

use mike::store::persistence::session_flow::{
    FlowAvailability, FlowComponent, FlowCursor, FlowLifecycle, FlowReadError, FlowRowId,
};

use super::support::{Fixture, ids};

#[tokio::test]
async fn tied_times_pages_retain_exact_ids_and_fixed_upper_bound() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let mut expected = Vec::new();
    for _ in 0..5 {
        expected.push(f.request(None, "primary").await?);
    }
    let first = f.page(FlowComponent::Requests, 2, None).await?;
    assert_eq!(ids(&first), expected[..2]);
    assert_eq!(
        first.coverage.lower_id,
        Some(FlowRowId::Bigint(expected[0]))
    );
    assert_eq!(
        first.coverage.upper_id,
        Some(FlowRowId::Bigint(expected[4]))
    );
    let cursor = first
        .next_cursor
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing cursor"))?;
    let late = f.request(None, "primary").await?;
    let second = f.page(FlowComponent::Requests, 2, Some(cursor)).await?;
    assert_eq!(
        second.coverage.lower_id,
        Some(FlowRowId::Bigint(expected[2]))
    );
    let third = f
        .page(FlowComponent::Requests, 2, second.next_cursor.as_ref())
        .await?;
    let actual = [ids(&first), ids(&second), ids(&third)].concat();
    assert_eq!(actual, expected);
    assert!(third.coverage.exhausted && third.next_cursor.is_none());
    expected.push(late);
    assert_eq!(
        ids(&f.page(FlowComponent::Requests, 10, None).await?),
        expected
    );
    f.cleanup().await
}

#[tokio::test]
async fn deleted_cursor_row_and_late_closure_require_replacement() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let first_id = f.request(None, "primary").await?;
    let second_id = f.request(None, "primary").await?;
    let third_id = f.request(None, "primary").await?;
    let first = f.page(FlowComponent::Requests, 1, None).await?;
    sqlx::query("DELETE FROM requests.requests WHERE id = $1")
        .bind(first_id)
        .execute(f.handle.pool())
        .await?;
    sqlx::query("UPDATE requests.requests SET completed_at = '2026-09-10T12:00:03Z', status_id = 3 WHERE id = $1")
        .bind(second_id).execute(f.handle.pool()).await?;
    let rest = f
        .page(FlowComponent::Requests, 10, first.next_cursor.as_ref())
        .await?;
    assert_eq!(ids(&rest), vec![second_id, third_id]);
    assert!(
        rest.activities
            .iter()
            .any(|a| a.source.row_id == FlowRowId::Bigint(second_id)
                && a.lifecycle == FlowLifecycle::Completed)
    );
    assert_eq!(
        ids(&f.page(FlowComponent::Requests, 10, None).await?),
        vec![second_id, third_id]
    );
    f.cleanup().await
}

#[tokio::test]
async fn unknown_unadmitted_future_and_empty_are_distinct() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    assert!(
        f.handle
            .fetch_session_flow(Uuid::new_v4(), FlowComponent::Requests, 10, None)
            .await?
            .is_none()
    );
    let empty = f.page(FlowComponent::Requests, 10, None).await?;
    assert!(matches!(
        empty.availability,
        FlowAvailability::Eligible { .. }
    ));
    assert!(empty.coverage.exhausted && empty.coverage.upper_id.is_none());
    let old = Uuid::new_v4();
    mike::store::persistence::id_resolvers::resolve_session_id(
        f.handle.pool(),
        &old.to_string(),
        None,
        None,
    )
    .await?;
    let old = f
        .handle
        .fetch_session_flow(old, FlowComponent::Requests, 10, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing old row"))?;
    assert_eq!(old.availability, FlowAvailability::NoAdmittedStart);
    sqlx::query("UPDATE sessions.sessions SET flow_capture_version = 2 WHERE id = $1")
        .bind(f.id)
        .execute(f.handle.pool())
        .await?;
    assert_eq!(
        f.page(FlowComponent::Requests, 10, None)
            .await?
            .availability,
        FlowAvailability::UnsupportedVersion { capture_version: 2 }
    );
    f.cleanup().await
}

#[tokio::test]
async fn cursor_scope_limits_and_embedding_cursor_fail_closed() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let valid = FlowCursor {
        version: 1,
        session_id: f.session,
        component: FlowComponent::Requests,
        filter: "all".into(),
        after_id: 1,
        upper_id: 2,
    };
    let mut invalid = vec![];
    let mut c = valid.clone();
    c.session_id = Uuid::new_v4();
    invalid.push(c);
    let mut c = valid.clone();
    c.component = FlowComponent::Tools;
    invalid.push(c);
    let mut c = valid.clone();
    c.version = 2;
    invalid.push(c);
    let mut c = valid.clone();
    c.filter = "body".into();
    invalid.push(c);
    let mut c = valid.clone();
    c.after_id = 0;
    invalid.push(c);
    let mut c = valid;
    c.after_id = 3;
    invalid.push(c);
    for cursor in invalid {
        assert!(matches!(
            f.handle
                .fetch_session_flow(f.session, FlowComponent::Requests, 10, Some(&cursor))
                .await,
            Err(FlowReadError::InvalidCursor)
        ));
    }
    for limit in [0, 501] {
        assert!(matches!(
            f.handle
                .fetch_session_flow(f.session, FlowComponent::Requests, limit, None)
                .await,
            Err(FlowReadError::InvalidCursor)
        ));
    }
    assert!(matches!(
        f.handle
            .fetch_session_flow(
                f.session,
                FlowComponent::Embeddings,
                10,
                Some(&FlowCursor {
                    version: 1,
                    session_id: f.session,
                    component: FlowComponent::Embeddings,
                    filter: "all".into(),
                    after_id: 1,
                    upper_id: 2
                })
            )
            .await,
        Err(FlowReadError::InvalidCursor)
    ));
    f.cleanup().await
}

#[tokio::test]
async fn late_commit_behind_cursor_is_recovered_by_fresh_traversal() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let first = f.request(None, "primary").await?;
    let mut late_tx = f.handle.pool().begin().await?;
    let late: i64 = sqlx::query_scalar(
        "INSERT INTO requests.requests
        (session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
         status_id, params, last_user_message, human_prompt, turn_type_id, turn_origin_id)
        SELECT session_id, project_id, method_id, path, is_streaming, message_count, is_tool_loop,
               status_id, params, last_user_message, human_prompt, turn_type_id, turn_origin_id
        FROM requests.requests WHERE id = $1 RETURNING id",
    )
    .bind(first)
    .fetch_one(&mut *late_tx)
    .await?;
    let third = f.request(None, "primary").await?;
    let fourth = f.request(None, "primary").await?;
    let page = f.page(FlowComponent::Requests, 2, None).await?;
    assert_eq!(ids(&page), vec![first, third]);
    late_tx.commit().await?;
    let continuation = f
        .page(FlowComponent::Requests, 2, page.next_cursor.as_ref())
        .await?;
    assert_eq!(ids(&continuation), vec![fourth]);
    assert_eq!(continuation.coverage.capture_complete, None);
    assert_eq!(
        ids(&f.page(FlowComponent::Requests, 10, None).await?),
        vec![first, late, third, fourth]
    );
    f.cleanup().await
}
