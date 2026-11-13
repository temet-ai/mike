use crate::store::persistence::knowledge_repo::KnowledgePersist as _;

use crate::store::models::RequestStatus;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::request_write::IdMode;
use crate::store::persistence::test_support;

const BACKFILL: &str = include_str!(
    "../../../mike-base/src/store/persistence/migrations/20261111170200_operator_turns_tool_origin.sql"
);

const TOOL_PROMPTS: [&str; 6] = [
    "A session-scoped Stop hook is now active with condition: \"tests pass\". Keep working until it holds.",
    "The tether plugin sent a message:\nMail from mike: Your task brief from mike (launch 1704).",
    "The fleet-comms plugin sent a message:\nMail from seat rust-T1.",
    "Perform a web search for the query: rust sqlx raw_sql transactions",
    "You score how well a stand-in predicted what Dawid would answer. Score from 0 to 1.",
    "Coordinator wake for fleet open-issues (0c16da5b): read the digest and act.",
];

const OPERATOR_PROMPT: &str = "Why did the tether plugin sent a message: line show up in recall?";

async fn capture(handle: &PersistHandle, raw_id: u64, prompt: &str) -> anyhow::Result<String> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let mut req = crate::store::tests::make_test_request(
        Some(crate::store::models::RequestId::from_raw(raw_id)),
        &session_id,
    );
    req.status = RequestStatus::Complete;
    req.completed_at = Some(req.created_at);
    req.last_user_message = prompt.to_string();
    req.human_prompt = prompt.to_string();
    req.turn_type = "user_prompt".to_string();
    req.turn_origin = "primary".to_string();
    handle
        .insert_request_initial(&req, true, IdMode::Ensure)
        .await?;
    let session_db_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(&session_id)
            .fetch_one(handle.pool())
            .await?;
    crate::store::operator_turns::record_prompt_kind_classification(
        Some(handle),
        crate::store::operator_turns::PromptKindClassificationInput {
            turn_origin: &crate::extract::request::TurnOrigin::Primary,
            turn_type: &crate::extract::request::TurnType::UserPrompt,
            request_id: req.id,
            session_id: &session_id,
            session_db_id: Some(session_db_id),
            human_prompt: prompt,
            last_user_message: prompt,
            author: "operator",
            has_agent_id: false,
        },
    )
    .await;
    handle.snapshot_request(&req).await?;
    Ok(session_id)
}

async fn stored_origin(
    pool: &sqlx::PgPool,
    session_id: &str,
) -> anyhow::Result<Vec<(Option<String>, Option<String>)>> {
    let rows: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT t.opened_by, k.code
           FROM memory.operator_turns t
           JOIN sessions.sessions s ON s.id = t.session_id
           LEFT JOIN prompt_kinds.classifications c ON c.request_id = t.start_request_id
           LEFT JOIN prompt_kinds.kinds k ON k.id = c.kind_id
          WHERE s.session_uuid = $1::text::uuid",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[tokio::test]
async fn capture_tool_written_prompt_stores_machine_turn_with_tool_kind() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    for (idx, prompt) in TOOL_PROMPTS.iter().enumerate() {
        let session_id = capture(&handle, 93_100 + idx as u64, prompt).await?;
        let rows = stored_origin(handle.pool(), &session_id).await?;
        assert_eq!(
            rows,
            vec![(Some("machine".to_string()), Some("tool-prompt".to_string()))],
            "prompt: {prompt}"
        );
    }
    let session_id = capture(&handle, 93_199, OPERATOR_PROMPT).await?;
    assert_eq!(
        stored_origin(handle.pool(), &session_id).await?,
        vec![(Some("operator".to_string()), Some("operator".to_string()))]
    );
    db.cleanup().await?;
    Ok(())
}

struct Seeded {
    project: &'static str,
    tool_turns: Vec<i64>,
    operator_turn: i64,
}

async fn seed_pre_migration_rows(pool: &sqlx::PgPool) -> anyhow::Result<Seeded> {
    let project = "w-tool-origin-backfill";
    let session = "00000000-0000-4000-8000-0000000000f1";
    let project_id = test_support::seed_project(pool, project).await?;
    test_support::seed_two_sessions(
        pool,
        session,
        "00000000-0000-4000-8000-0000000000f2",
        project_id,
    )
    .await?;
    test_support::seed_session_turns(pool, session, project_id, TOOL_PROMPTS.len() as i64 + 1)
        .await?;
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM memory.operator_turns WHERE project_id = $1 ORDER BY id",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    let Some((&operator_turn, tool_turns)) = ids.split_last() else {
        anyhow::bail!("no seeded turns");
    };
    let mut prompts: Vec<String> = TOOL_PROMPTS.iter().map(|p| (*p).to_string()).collect();
    prompts.push(format!("zebra {OPERATOR_PROMPT}"));
    sqlx::query(
        "WITH p AS (SELECT * FROM unnest($1::bigint[], $2::text[]) AS p(turn_id, prompt))
         UPDATE memory.operator_turns t
            SET opened_by = 'operator', user_prompt = p.prompt || ' zebra',
                closed_at = NOW() - interval '10 hours'
           FROM p WHERE t.id = p.turn_id",
    )
    .bind(&ids)
    .bind(&prompts)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE requests.requests r SET human_prompt = t.user_prompt, last_user_message = t.user_prompt
           FROM memory.operator_turns t WHERE t.start_request_id = r.id AND t.id = ANY($1)",
    )
    .bind(&ids)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO prompt_kinds.classifications (request_id, session_id, kind_id, classifier_version)
         SELECT t.start_request_id, t.session_id, k.id, 'genuine-v2'
           FROM memory.operator_turns t, prompt_kinds.kinds k
          WHERE t.id = ANY($1) AND k.code = 'operator'",
    )
    .bind(&ids)
    .execute(pool)
    .await?;
    Ok(Seeded {
        project,
        tool_turns: tool_turns.to_vec(),
        operator_turn,
    })
}

async fn origins(pool: &sqlx::PgPool, ids: &[i64]) -> anyhow::Result<Vec<(i64, String, String)>> {
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT t.id, t.opened_by, k.code
           FROM memory.operator_turns t
           JOIN prompt_kinds.classifications c ON c.request_id = t.start_request_id
           JOIN prompt_kinds.kinds k ON k.id = c.kind_id
          WHERE t.id = ANY($1) ORDER BY t.id",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[tokio::test]
async fn backfill_reclassifies_tool_written_turns_once_and_recall_skips_them() -> anyhow::Result<()>
{
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let pool = handle.pool();
    let seeded = seed_pre_migration_rows(pool).await?;
    let mut all = seeded.tool_turns.clone();
    all.push(seeded.operator_turn);

    sqlx::raw_sql(BACKFILL).execute(pool).await?;
    let after_first = origins(pool, &all).await?;
    for (id, opened_by, kind) in &after_first {
        if *id == seeded.operator_turn {
            assert_eq!(
                (opened_by.as_str(), kind.as_str()),
                ("operator", "operator")
            );
        } else {
            assert_eq!(
                (opened_by.as_str(), kind.as_str()),
                ("machine", "tool-prompt"),
                "turn {id}"
            );
        }
    }
    sqlx::raw_sql(BACKFILL).execute(pool).await?;
    assert_eq!(
        origins(pool, &all).await?,
        after_first,
        "a second run changes nothing"
    );

    let project = handle
        .knowledge()
        .resolve_knowledge_project(seeded.project)
        .await?;
    let recalled: Vec<i64> = handle
        .knowledge()
        .recall_turns_by_fts(&project, "zebra", 20, None)
        .await?
        .iter()
        .map(|row| row.turn_id)
        .collect();
    assert_eq!(recalled, vec![seeded.operator_turn]);
    db.cleanup().await?;
    Ok(())
}
