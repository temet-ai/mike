use serde_json::json;
use uuid::Uuid;

use super::fixture_tests::{Fixture, SEARCH_TOOL};
use super::mark_used::{USED_BATCH, WINDOW_TURNS};
use crate::store::persistence::test_support::seed_session_turns;

const READ_TOOL: &str = "mcp__mike__read_project_knowledge";
const TURN_TOOL: &str = "mcp__mike__get_operator_turn";

async fn search_in_request(
    fixture: &Fixture,
    request: usize,
    query: &str,
    refs: &[&str],
) -> anyhow::Result<i64> {
    fixture
        .tool_use(
            request,
            SEARCH_TOOL,
            json!({"project": "p", "query": query}),
        )
        .await?;
    fixture.record(Some(fixture.session), query, refs).await
}

#[tokio::test]
async fn mark_used_when_the_session_reads_a_returned_reference_marks_it_used() -> anyhow::Result<()>
{
    let fixture = Fixture::boot(6).await?;
    let event = search_in_request(&fixture, 1, "cache rules", &["lesson:a", "artifact:5"]).await?;
    fixture
        .tool_use(2, READ_TOOL, json!({"reference": "artifact:5"}))
        .await?;

    let pass = fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;

    assert_eq!(pass.updated, 1);
    assert_eq!(pass.with_used, 1);
    assert_eq!(
        fixture.used(event).await?,
        (vec!["artifact:5".to_owned()], false)
    );
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_the_session_opens_a_returned_turn_marks_it_used() -> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let event = search_in_request(&fixture, 0, "old failure", &["turn:9001", "turn:9002"]).await?;
    fixture.tool_use(1, TURN_TOOL, json!({"id": 9001})).await?;

    fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;

    assert_eq!(fixture.used(event).await?.0, vec!["turn:9001".to_owned()]);
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_a_read_names_a_reference_that_was_not_returned_leaves_it_unused()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let event = search_in_request(&fixture, 1, "cache rules", &["lesson:a"]).await?;
    fixture
        .tool_use(2, READ_TOOL, json!({"reference": "lesson:other"}))
        .await?;

    let pass = fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;

    assert_eq!(pass.with_used, 0);
    assert!(fixture.used(event).await?.0.is_empty());
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_the_read_is_in_the_same_request_as_the_search_leaves_it_unused()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let event = search_in_request(&fixture, 1, "cache rules", &["lesson:a"]).await?;
    fixture
        .tool_use(1, READ_TOOL, json!({"reference": "lesson:a"}))
        .await?;

    fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;

    assert!(fixture.used(event).await?.0.is_empty());
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_the_final_reply_names_a_reference_marks_it_used() -> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let event = search_in_request(&fixture, 1, "cache rules", &["lesson:a", "turn:12"]).await?;
    fixture
        .final_reply(
            2,
            "Following lesson:a, the cache stays warm. See also turn:123.",
        )
        .await?;

    fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;

    assert_eq!(fixture.used(event).await?.0, vec!["lesson:a".to_owned()]);
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_the_next_five_turns_have_closed_finalises_the_event() -> anyhow::Result<()>
{
    let fixture = Fixture::boot(8).await?;
    let event = search_in_request(&fixture, 1, "cache rules", &["lesson:a"]).await?;
    fixture
        .tool_use(2, READ_TOOL, json!({"reference": "lesson:a"}))
        .await?;

    let pass = fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;

    assert_eq!(pass.finalised, 1);
    assert_eq!(
        fixture.used(event).await?,
        (vec!["lesson:a".to_owned()], true)
    );
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_the_read_comes_after_the_window_closed_leaves_it_unused()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(9).await?;
    let event = search_in_request(&fixture, 1, "cache rules", &["lesson:a"]).await?;
    fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;
    assert!(fixture.used(event).await?.1);
    fixture
        .tool_use(8, READ_TOOL, json!({"reference": "lesson:a"}))
        .await?;

    fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;

    assert_eq!(fixture.used(event).await?, (Vec::<String>::new(), true));
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_fewer_turns_follow_keeps_the_event_open_for_a_later_read()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(3).await?;
    let event = search_in_request(&fixture, 1, "cache rules", &["lesson:a"]).await?;
    let first = fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;
    assert_eq!(first.finalised, 0);
    fixture
        .tool_use(2, READ_TOOL, json!({"reference": "lesson:a"}))
        .await?;

    fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;

    assert_eq!(
        fixture.used(event).await?,
        (vec!["lesson:a".to_owned()], false)
    );
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_an_open_event_is_older_than_the_limit_finalises_it() -> anyhow::Result<()> {
    let fixture = Fixture::boot(2).await?;
    let event = search_in_request(&fixture, 1, "cache rules", &["lesson:a"]).await?;

    let pass = fixture
        .handle
        .mark_used_within(USED_BATCH, WINDOW_TURNS, -1.0)
        .await?;

    assert_eq!(pass.finalised, 1);
    assert!(fixture.used(event).await?.1);
    fixture.cleanup().await
}

struct Elsewhere {
    uuid: Uuid,
    session_row: i64,
    turn: i64,
}

async fn elsewhere(fixture: &Fixture) -> anyhow::Result<Elsewhere> {
    let pool = fixture.handle.pool();
    let project: i64 =
        sqlx::query_scalar("SELECT project_id FROM sessions.sessions WHERE session_uuid = $1")
            .bind(fixture.session)
            .fetch_one(pool)
            .await?;
    let uuid = Uuid::new_v4();
    let session_row: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, now(), now()) RETURNING id",
    )
    .bind(uuid)
    .bind(project)
    .fetch_one(pool)
    .await?;
    seed_session_turns(pool, &uuid.to_string(), project, 1).await?;
    let turn: i64 = sqlx::query_scalar(
        "SELECT t.id FROM memory.operator_turns t WHERE t.session_id = $1 ORDER BY t.id LIMIT 1",
    )
    .bind(session_row)
    .fetch_one(pool)
    .await?;
    Ok(Elsewhere {
        uuid,
        session_row,
        turn,
    })
}

async fn verdict_of(fixture: &Fixture, other: &Elsewhere) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO memory.turn_verdicts
            (operator_turn_id, session_id, mode, rubric_version, input_builder_version, model,
             confidence, rationale, unmet_ask, evidence_quality, as_of, input_text, reaction,
             delivery, source, created_at)
         VALUES ($1, $2, 'live', 'r1', 'i1', 'test', 0.9, 'why', 'ask', 'asserted', now(), '',
                 'corrected', 'partial', 'llm', now())
         RETURNING id",
    )
    .bind(other.turn)
    .bind(other.session_row)
    .fetch_one(fixture.handle.pool())
    .await?)
}

#[tokio::test]
async fn mark_used_when_the_session_opens_the_session_a_returned_turn_came_from_marks_it_used()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let other = elsewhere(&fixture).await?;
    let turn = format!("turn:{}", other.turn);
    let event = search_in_request(&fixture, 1, "old failure", &[&turn, "turn:1"]).await?;
    fixture
        .tool_use(
            2,
            "mcp__mike__get_session_overview",
            json!({"id": other.uuid.to_string()}),
        )
        .await?;

    fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;

    assert_eq!(fixture.used(event).await?.0, vec![turn]);
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_a_later_call_names_the_source_session_in_session_id_marks_it_used()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let other = elsewhere(&fixture).await?;
    let turn = format!("turn:{}", other.turn);
    let event = search_in_request(&fixture, 1, "old failure", &[&turn]).await?;
    fixture
        .tool_use(
            2,
            "mcp__mike__find_text",
            json!({"session_id": other.uuid.to_string(), "query": "x"}),
        )
        .await?;

    fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;

    assert_eq!(fixture.used(event).await?.0, vec![turn]);
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_the_session_opens_the_session_of_a_returned_verdict_marks_both_used()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let other = elsewhere(&fixture).await?;
    let verdict = verdict_of(&fixture, &other).await?;
    let (correction, unmet) = (
        format!("correction:{verdict}"),
        format!("unmet_ask:{verdict}"),
    );
    let event =
        search_in_request(&fixture, 1, "ask", &[&correction, &unmet, "correction:1"]).await?;
    fixture
        .tool_use(
            2,
            "mcp__mike__get_session",
            json!({"id": other.uuid.to_string()}),
        )
        .await?;

    fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;

    let mut expected = vec![correction, unmet];
    expected.sort();
    assert_eq!(fixture.used(event).await?.0, expected);
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_a_call_names_an_unrelated_session_marks_nothing() -> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let other = elsewhere(&fixture).await?;
    let event = search_in_request(
        &fixture,
        1,
        "old failure",
        &[&format!("turn:{}", other.turn)],
    )
    .await?;
    fixture
        .tool_use(
            2,
            "mcp__mike__find_text",
            json!({"session_id": Uuid::new_v4().to_string()}),
        )
        .await?;

    fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;

    assert!(fixture.used(event).await?.0.is_empty());
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_a_call_names_the_calling_session_marks_nothing() -> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let other = elsewhere(&fixture).await?;
    let own_turn: i64 = sqlx::query_scalar(
        "SELECT t.id FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
          WHERE s.session_uuid = $1 ORDER BY t.id LIMIT 1",
    )
    .bind(fixture.session)
    .fetch_one(fixture.handle.pool())
    .await?;
    let returned = [format!("turn:{}", other.turn), format!("turn:{own_turn}")];
    let event = search_in_request(
        &fixture,
        1,
        "old failure",
        &[returned[0].as_str(), returned[1].as_str()],
    )
    .await?;
    fixture
        .tool_use(
            2,
            "mcp__mike__get_session_overview",
            json!({"id": fixture.session.to_string(), "session_id": fixture.session.to_string()}),
        )
        .await?;

    fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;

    assert!(fixture.used(event).await?.0.is_empty());
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_a_read_names_the_bare_id_or_an_eight_character_prefix_marks_it_used()
-> anyhow::Result<()> {
    let id = Uuid::new_v4().to_string();
    for read in [id.clone(), id[..8].to_owned()] {
        let fixture = Fixture::boot(6).await?;
        let lesson = format!("lesson:{id}");
        let event = search_in_request(&fixture, 1, "cache rules", &[&lesson, "lesson:b"]).await?;
        fixture
            .tool_use(2, READ_TOOL, json!({"reference": read}))
            .await?;

        fixture
            .handle
            .mark_used_search_references(USED_BATCH)
            .await?;

        assert_eq!(fixture.used(event).await?.0, vec![lesson], "{read}");
        fixture.cleanup().await?;
    }
    Ok(())
}

#[tokio::test]
async fn mark_used_when_a_read_names_a_prefix_shorter_than_eight_characters_marks_nothing()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let id = Uuid::new_v4().to_string();
    let event = search_in_request(&fixture, 1, "cache rules", &[&format!("lesson:{id}")]).await?;
    fixture
        .tool_use(2, READ_TOOL, json!({"reference": &id[..7]}))
        .await?;

    fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;

    assert!(fixture.used(event).await?.0.is_empty());
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_another_session_reads_the_reference_leaves_it_unused() -> anyhow::Result<()>
{
    let fixture = Fixture::boot(4).await?;
    let event = search_in_request(&fixture, 1, "cache rules", &["lesson:a"]).await?;
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
                 SELECT gen_random_uuid(), project_id, now(), now() FROM sessions.sessions LIMIT 1",
    )
    .execute(fixture.handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO tools.uses (wire_tool_use_id, request_id, session_id, ord, tool_name, input)
         SELECT 'toolu_other', r.id, other.id, 1, $1, '{\"reference\":\"lesson:a\"}'::jsonb
           FROM requests.requests r, sessions.sessions other
          WHERE r.id = $2 AND other.id <> r.session_id",
    )
    .bind(READ_TOOL)
    .bind(fixture.requests[2])
    .execute(fixture.handle.pool())
    .await?;

    fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;

    assert!(fixture.used(event).await?.0.is_empty());
    fixture.cleanup().await
}
