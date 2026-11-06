use crate::test_db::{self, TestDb, TestDbHandle};

use mike::extract::request::{TurnOrigin, TurnType};
use mike::store::models::RequestId;
use mike::store::operator_turns::{
    PromptKindClassificationInput, record_prompt_kind_classification,
};
use mike::store::persistence::PersistHandle;
use mike::store::persistence::operator_turns::SessionOverviewFilter;
use mike::types::SortOrder;
use sqlx::PgPool;

const LIST_FOR_SESSION_LIMIT: i64 = 1000;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_for_session_caps_at_limit_with_truncated_flag() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = PgPool::connect(&db.database_url).await?;

    let project_id = test_db::seed_project(&pool, "test-w1.4-list-for-session-cap").await?;

    let pathological_session = "00000000-0000-4000-8000-000000005000";
    let exact_cap_session = "00000000-0000-4000-8000-000000001000";

    test_db::seed_two_sessions(&pool, pathological_session, exact_cap_session, project_id).await?;

    let over_cap_turns = LIST_FOR_SESSION_LIMIT * 2 + 500;
    test_db::seed_session_turns(&pool, pathological_session, project_id, over_cap_turns).await?;
    test_db::seed_session_turns(&pool, exact_cap_session, project_id, 1000).await?;

    let pathological_db_id = handle
        .resolve_session_db_id(pathological_session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("pathological session must exist"))?;
    let exact_cap_db_id = handle
        .resolve_session_db_id(exact_cap_session)
        .await?
        .ok_or_else(|| anyhow::anyhow!("exact-cap session must exist"))?;

    let (desc_rows, truncated) = handle
        .list_for_session_bounded(
            pathological_db_id,
            SortOrder::Desc,
            &SessionOverviewFilter::default(),
        )
        .await?;
    assert_eq!(
        desc_rows.len() as i64,
        LIST_FOR_SESSION_LIMIT,
        "pathological session must be capped at LIST_FOR_SESSION_LIMIT"
    );
    assert!(
        truncated,
        "pathological session over the cap must report truncated=true"
    );
    assert!(
        desc_rows
            .windows(2)
            .all(|w| w[0].start_request_id > w[1].start_request_id),
        "Desc rows must be strictly newest-first by start_request_id"
    );

    let (asc_rows, asc_truncated) = handle
        .list_for_session_bounded(
            pathological_db_id,
            SortOrder::Asc,
            &SessionOverviewFilter::default(),
        )
        .await?;
    assert_eq!(asc_rows.len() as i64, LIST_FOR_SESSION_LIMIT);
    assert!(asc_truncated, "Asc over the cap must also report truncated");
    let asc_max = asc_rows
        .last()
        .map(|r| r.start_request_id)
        .ok_or_else(|| anyhow::anyhow!("asc rows must be non-empty"))?;
    let desc_min = desc_rows
        .last()
        .map(|r| r.start_request_id)
        .ok_or_else(|| anyhow::anyhow!("desc rows must be non-empty"))?;
    assert!(
        desc_min > asc_max,
        "Desc must keep the NEWEST 1000 turns (its oldest kept row {desc_min} must be \
         newer than Asc's newest kept row {asc_max})"
    );

    let (rows, truncated) = handle
        .list_for_session_bounded(
            exact_cap_db_id,
            SortOrder::Desc,
            &SessionOverviewFilter::default(),
        )
        .await?;
    assert_eq!(
        rows.len() as i64,
        LIST_FOR_SESSION_LIMIT,
        "exact-cap session must return all LIST_FOR_SESSION_LIMIT rows"
    );
    assert!(
        !truncated,
        "exact-cap session must NOT report truncated when row count == LIST_FOR_SESSION_LIMIT"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

async fn seeded_request_fixture(
    db: &TestDbHandle,
    seed_index: u32,
) -> anyhow::Result<(PersistHandle, PgPool, String, i64, i64)> {
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = PgPool::connect(&db.database_url).await?;

    let project_id =
        test_db::seed_project(&pool, &format!("test-prompt-kind-{seed_index}")).await?;
    let session_uuid = format!("00000000-0000-4000-8000-{seed_index:012x}");
    let other_session_uuid = format!("00000000-0000-4000-8001-{seed_index:012x}");
    test_db::seed_two_sessions(&pool, &session_uuid, &other_session_uuid, project_id).await?;
    test_db::seed_session_turns(&pool, &session_uuid, project_id, 1).await?;

    let session_db_id = handle
        .resolve_session_db_id(&session_uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("seeded session must resolve"))?;
    let request_db_id: i64 =
        sqlx::query_scalar("SELECT id FROM requests.requests WHERE session_id = $1 LIMIT 1")
            .bind(session_db_id)
            .fetch_one(&pool)
            .await?;

    Ok((handle, pool, session_uuid, session_db_id, request_db_id))
}

async fn classification_count(pool: &PgPool, request_db_id: i64) -> anyhow::Result<i64> {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM prompt_kinds.classifications WHERE request_id = $1")
            .bind(request_db_id)
            .fetch_one(pool)
            .await?;
    Ok(count)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn record_prompt_kind_classification_writes_and_is_idempotent() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let (handle, pool, session_uuid, session_db_id, request_db_id) =
        seeded_request_fixture(&db, 1).await?;

    for _ in 0..2 {
        record_prompt_kind_classification(
            Some(&handle),
            PromptKindClassificationInput {
                turn_origin: &TurnOrigin::Primary,
                turn_type: &TurnType::UserPrompt,
                request_id: RequestId::from_raw(request_db_id as u64),
                session_id: &session_uuid,
                session_db_id: Some(session_db_id),
                human_prompt: "plain human prompt",
                last_user_message: "",
                author: "operator",
                has_agent_id: false,
            },
        )
        .await;
    }
    assert_eq!(
        classification_count(&pool, request_db_id).await?,
        1,
        "ON CONFLICT DO NOTHING must keep the row singular across repeat calls"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn record_prompt_kind_classification_skips_write_when_gate_fails() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let (handle, pool, session_uuid, session_db_id, request_db_id) =
        seeded_request_fixture(&db, 2).await?;

    record_prompt_kind_classification(
        Some(&handle),
        PromptKindClassificationInput {
            turn_origin: &TurnOrigin::Subagent,
            turn_type: &TurnType::UserPrompt,
            request_id: RequestId::from_raw(request_db_id as u64),
            session_id: &session_uuid,
            session_db_id: Some(session_db_id),
            human_prompt: "plain human prompt",
            last_user_message: "",
            author: "operator",
            has_agent_id: false,
        },
    )
    .await;
    assert_eq!(
        classification_count(&pool, request_db_id).await?,
        0,
        "a non-operator-turn-opener must never write a classification row"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn record_prompt_kind_classification_skips_write_when_session_db_id_is_none()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let (handle, pool, session_uuid, _session_db_id, request_db_id) =
        seeded_request_fixture(&db, 3).await?;

    record_prompt_kind_classification(
        Some(&handle),
        PromptKindClassificationInput {
            turn_origin: &TurnOrigin::Primary,
            turn_type: &TurnType::UserPrompt,
            request_id: RequestId::from_raw(request_db_id as u64),
            session_id: &session_uuid,
            session_db_id: None,
            human_prompt: "plain human prompt",
            last_user_message: "",
            author: "operator",
            has_agent_id: false,
        },
    )
    .await;
    assert_eq!(
        classification_count(&pool, request_db_id).await?,
        0,
        "an unresolved session id must never write a classification row"
    );

    pool.close().await;
    db.cleanup().await?;
    Ok(())
}
