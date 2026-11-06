use std::sync::Arc;

use super::{PersistError, PersistHandle};
use crate::store::models::{InterceptedRequest, RequestId};
use crate::store::persistence::request_write::IdMode;
use crate::store::persistence::test_support::TestDb;
use crate::types::ToolCall;

async fn shared_handle() -> anyhow::Result<Option<PersistHandle>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = test_db.acquire_shared().await?;
    Ok(Some(PersistHandle::connect(&db.database_url).await?))
}

async fn insert(
    handle: &PersistHandle,
    req: &InterceptedRequest,
    is_new_session: bool,
) -> anyhow::Result<()> {
    let mut attempt = 0;
    loop {
        match handle
            .insert_request_initial(req, is_new_session, IdMode::Ensure)
            .await
        {
            Ok(_) => return Ok(()),
            Err(PersistError::Database(sqlx::Error::RowNotFound)) if attempt < 5 => attempt += 1,
            Err(e) => return Err(e.into()),
        }
    }
}

fn body_with_messages(count: usize) -> bytes::Bytes {
    let messages: Vec<_> = (0..count)
        .map(|i| serde_json::json!({"role": "user", "content": format!("message {i}")}))
        .collect();
    bytes::Bytes::from(serde_json::json!({ "messages": messages }).to_string())
}

fn numbered_request(raw_id: u64, session_id: &str, message_count: usize) -> InterceptedRequest {
    let mut req =
        crate::store::tests::make_test_request(Some(RequestId::from_raw(raw_id)), session_id);
    req.request_body = Some(body_with_messages(message_count));
    req
}

async fn session_db_id(handle: &PersistHandle, session_id: &str) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(session_id)
            .fetch_one(handle.pool())
            .await?,
    )
}

#[tokio::test]
async fn concurrent_requests_in_one_session_get_distinct_gap_free_request_numbers()
-> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let handle = Arc::new(handle);
    let session_id = uuid::Uuid::new_v4().to_string();
    let first = numbered_request(362_000, &session_id, 1);
    insert(&handle, &first, true).await?;

    let mut inserts = tokio::task::JoinSet::new();
    for offset in 1..=16_u64 {
        let handle = Arc::clone(&handle);
        let req = numbered_request(362_000 + offset, &session_id, 1 + offset as usize);
        inserts.spawn(async move { insert(&handle, &req, false).await });
    }
    while let Some(joined) = inserts.join_next().await {
        joined??;
    }

    let session = session_db_id(&handle, &session_id).await?;
    let numbers: Vec<i64> = sqlx::query_scalar(
        "SELECT request_number FROM requests.requests
          WHERE session_id = $1 ORDER BY request_number",
    )
    .bind(session)
    .fetch_all(handle.pool())
    .await?;
    let expected: Vec<i64> = (1..=17).collect();
    assert_eq!(
        numbers, expected,
        "17 requests in one session must hold the numbers 1..=17 exactly once each"
    );
    Ok(())
}

#[tokio::test]
async fn rerunning_the_stamp_for_a_request_keeps_its_request_number() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let first = numbered_request(362_100, &session_id, 1);
    let second = numbered_request(362_101, &session_id, 2);
    insert(&handle, &first, true).await?;
    insert(&handle, &second, false).await?;
    insert(&handle, &second, false).await?;

    let number: Option<i64> =
        sqlx::query_scalar("SELECT request_number FROM requests.requests WHERE id = $1")
            .bind(362_101_i64)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(number, Some(2));
    Ok(())
}

async fn session_with_two_unclaimed_instances(
    handle: &PersistHandle,
    session_id: &str,
    persona: &str,
    raw_id: u64,
) -> anyhow::Result<(i64, Vec<i64>)> {
    let mut orchestrator =
        crate::store::tests::make_test_request(Some(RequestId::from_raw(raw_id)), session_id);
    orchestrator.turn_origin = "primary_first".to_string();
    orchestrator.tool_calls = (0..2)
        .map(|n| ToolCall {
            tool_id: format!("tu-{session_id}-{n}"),
            name: "Agent".to_string(),
            input_json: serde_json::json!({"subagent_type": "general-purpose"}).to_string(),
            complete: true,
        })
        .collect();
    insert(handle, &orchestrator, true).await?;
    handle.create_instances_for_dispatches(session_id).await?;

    let session = session_db_id(handle, session_id).await?;
    let instances: Vec<i64> = sqlx::query_scalar(
        "UPDATE agents.instances
            SET subagent_persona = $2, first_request_id = NULL, started_at = NULL
          WHERE session_id = $1
          RETURNING id",
    )
    .bind(session)
    .bind(persona)
    .fetch_all(handle.pool())
    .await?;
    Ok((session, instances))
}

fn subagent_request(
    raw_id: u64,
    session_id: &str,
    persona: &str,
    agent_id: Option<&str>,
) -> InterceptedRequest {
    let mut req =
        crate::store::tests::make_test_request(Some(RequestId::from_raw(raw_id)), session_id);
    req.turn_origin = "subagent".to_string();
    req.subagent_persona = Some(persona.to_string());
    req.claude_code_agent_id = agent_id.map(str::to_string);
    req
}

async fn bound_instance(handle: &PersistHandle, raw_id: i64) -> anyhow::Result<Option<i64>> {
    Ok(
        sqlx::query_scalar("SELECT agent_instance_id FROM requests.requests WHERE id = $1")
            .bind(raw_id)
            .fetch_one(handle.pool())
            .await?,
    )
}

#[tokio::test]
async fn concurrent_agents_sharing_a_persona_bind_to_their_own_instances() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let persona = "General-purpose helper";
    let (_session, instances) =
        session_with_two_unclaimed_instances(&handle, &session_id, persona, 361_000).await?;
    assert_eq!(
        instances.len(),
        2,
        "two dispatches must yield two instances"
    );

    let sequence = [
        (361_001_u64, "agent-a"),
        (361_002, "agent-b"),
        (361_003, "agent-a"),
        (361_004, "agent-b"),
        (361_005, "agent-a"),
    ];
    for (raw_id, agent_id) in sequence {
        let req = subagent_request(raw_id, &session_id, persona, Some(agent_id));
        insert(&handle, &req, false).await?;
    }

    let a_first = bound_instance(&handle, 361_001).await?;
    let b_first = bound_instance(&handle, 361_002).await?;
    assert!(
        a_first.is_some() && b_first.is_some(),
        "both agents must bind"
    );
    assert_ne!(a_first, b_first, "two agent ids must not share an instance");
    assert_eq!(bound_instance(&handle, 361_003).await?, a_first);
    assert_eq!(bound_instance(&handle, 361_005).await?, a_first);
    assert_eq!(bound_instance(&handle, 361_004).await?, b_first);

    let merged: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM (
             SELECT agent_instance_id
               FROM requests.requests
              WHERE agent_instance_id = ANY($1) AND claude_code_agent_id IS NOT NULL
              GROUP BY agent_instance_id
             HAVING COUNT(DISTINCT claude_code_agent_id) > 1
         ) merged",
    )
    .bind(&instances)
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(merged, 0, "no instance may carry more than one agent id");
    Ok(())
}

#[tokio::test]
async fn parallel_first_requests_of_different_agents_claim_different_instances()
-> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let handle = Arc::new(handle);
    let session_id = uuid::Uuid::new_v4().to_string();
    let persona = "Parallel general-purpose helper";
    session_with_two_unclaimed_instances(&handle, &session_id, persona, 361_100).await?;

    let mut inserts = tokio::task::JoinSet::new();
    for (raw_id, agent_id) in [(361_101_u64, "agent-x"), (361_102, "agent-y")] {
        let handle = Arc::clone(&handle);
        let req = subagent_request(raw_id, &session_id, persona, Some(agent_id));
        inserts.spawn(async move { insert(&handle, &req, false).await });
    }
    while let Some(joined) = inserts.join_next().await {
        joined??;
    }

    let x = bound_instance(&handle, 361_101).await?;
    let y = bound_instance(&handle, 361_102).await?;
    assert!(
        x.is_some() && y.is_some(),
        "both agents must bind: {x:?} {y:?}"
    );
    assert_ne!(x, y, "simultaneous first requests must not merge");
    Ok(())
}

#[tokio::test]
async fn request_without_an_agent_id_still_binds_by_persona() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let persona = "Persona-only helper";
    let (_session, instances) =
        session_with_two_unclaimed_instances(&handle, &session_id, persona, 361_200).await?;

    let req = subagent_request(361_201, &session_id, persona, None);
    insert(&handle, &req, false).await?;

    let bound = bound_instance(&handle, 361_201).await?;
    assert!(
        bound.is_some_and(|id| instances.contains(&id)),
        "a request with no agent id must keep the persona fallback: {bound:?}"
    );
    Ok(())
}

#[tokio::test]
async fn proxy_tool_followup_of_a_subagent_keeps_its_agent_id_and_never_opens_an_operator_turn()
-> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let session_id = uuid::Uuid::new_v4().to_string();
    let mut followup =
        crate::store::tests::make_test_request(Some(RequestId::from_raw(360_001)), &session_id);
    followup.turn_origin = "subagent".to_string();
    followup.turn_type = "tool_loop".to_string();
    followup.claude_code_agent_id = Some("agent-360".to_string());
    followup.human_prompt = "proxy tool result for 'mike_session_roster'".to_string();
    insert(&handle, &followup, true).await?;

    let mut main_lane =
        crate::store::tests::make_test_request(Some(RequestId::from_raw(360_002)), &session_id);
    main_lane.turn_type = "tool_loop".to_string();
    main_lane.human_prompt = "proxy tool result for 'mike_session_roster'".to_string();
    insert(&handle, &main_lane, false).await?;

    let rows: Vec<(i64, Option<String>, String)> = sqlx::query_as(
        "SELECT r.id, r.claude_code_agent_id, ak.code
           FROM requests.requests r
           JOIN requests.author_kinds ak ON ak.id = r.author_kind_id
          WHERE r.id IN (360001, 360002)
          ORDER BY r.id",
    )
    .fetch_all(handle.pool())
    .await?;
    assert_eq!(
        rows,
        vec![
            (360_001, Some("agent-360".to_string()), "agent".to_string()),
            (360_002, None, "harness".to_string()),
        ]
    );
    Ok(())
}
