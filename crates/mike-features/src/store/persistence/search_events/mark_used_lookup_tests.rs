use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use serde_json::{Value, json};
use uuid::Uuid;

use super::fixture_tests::Fixture;
use super::mark_used::USED_BATCH;

const CONVERSATION_TOOL: &str = "search_session_content";
const TEXT_TOOL: &str = "find_text";
const OPEN_SESSION_TOOL: &str = "mcp__mike__get_session_overview";
const OPEN_REQUEST_TOOL: &str = "mcp__mike__get_request_body";
const TURN_TOOL: &str = "mcp__mike__get_operator_turn";

fn sessions_returned(sessions: &[Uuid]) -> Value {
    Value::Array(
        sessions
            .iter()
            .enumerate()
            .map(|(index, session)| {
                json!({"reference": format!("session:{session}"), "matches": index + 1})
            })
            .collect(),
    )
}

fn requests_returned(requests: &[i64]) -> Value {
    Value::Array(
        requests
            .iter()
            .map(|request| json!({"reference": format!("request:{request}")}))
            .collect(),
    )
}

async fn conversation_search(
    fixture: &Fixture,
    query: &str,
    sessions: &[Uuid],
) -> anyhow::Result<i64> {
    fixture
        .tool_use(1, "mcp__mike__search_session_content", json!({"q": query}))
        .await?;
    fixture
        .record_returned(CONVERSATION_TOOL, query, &sessions_returned(sessions))
        .await
}

async fn text_search(fixture: &Fixture, needle: &str, requests: &[i64]) -> anyhow::Result<i64> {
    fixture
        .tool_use(1, "mcp__mike__find_text", json!({"needle": needle}))
        .await?;
    fixture
        .record_returned(TEXT_TOOL, needle, &requests_returned(requests))
        .await
}

async fn mark(fixture: &Fixture) -> anyhow::Result<()> {
    fixture
        .handle
        .knowledge()
        .mark_used_search_references(USED_BATCH)
        .await?;
    Ok(())
}

#[tokio::test]
async fn mark_used_when_the_session_opens_a_returned_session_marks_only_that_session()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let (opened, passed_over) = (Uuid::new_v4(), Uuid::new_v4());
    let event = conversation_search(&fixture, "cache rules", &[opened, passed_over]).await?;
    fixture
        .tool_use(
            2,
            OPEN_SESSION_TOOL,
            json!({"session_id": opened.to_string()}),
        )
        .await?;

    mark(&fixture).await?;

    assert_eq!(fixture.event(event).await?.1, Some(fixture.requests[1]));
    assert_eq!(
        fixture.used(event).await?.0,
        vec![format!("session:{opened}")]
    );
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_the_session_opens_a_session_the_search_did_not_return_marks_nothing()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let event = conversation_search(&fixture, "cache rules", &[Uuid::new_v4()]).await?;
    fixture
        .tool_use(
            2,
            OPEN_SESSION_TOOL,
            json!({"session_id": Uuid::new_v4().to_string()}),
        )
        .await?;

    mark(&fixture).await?;

    assert!(fixture.used(event).await?.0.is_empty());
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_a_returned_session_is_opened_by_id_marks_it_used() -> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let found = Uuid::new_v4();
    let event = conversation_search(&fixture, "cache rules", &[found]).await?;
    fixture
        .tool_use(
            2,
            "mcp__mike__get_session",
            json!({"id": found.to_string().to_uppercase()}),
        )
        .await?;

    mark(&fixture).await?;

    assert_eq!(
        fixture.used(event).await?.0,
        vec![format!("session:{found}")]
    );
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_the_search_returns_the_calling_session_and_it_names_itself_marks_nothing()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let event = conversation_search(&fixture, "cache rules", &[fixture.session]).await?;
    fixture
        .tool_use(
            2,
            "mcp__mike__find_text",
            json!({"session_id": fixture.session.to_string(), "needle": "x"}),
        )
        .await?;

    mark(&fixture).await?;

    assert!(fixture.used(event).await?.0.is_empty());
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_the_session_opens_a_returned_request_by_request_id_marks_only_that_request()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let event = text_search(&fixture, "needle-x", &[900_001, 900_002]).await?;
    fixture
        .tool_use(2, OPEN_REQUEST_TOOL, json!({"request_id": 900_002}))
        .await?;

    mark(&fixture).await?;

    assert_eq!(fixture.event(event).await?.1, Some(fixture.requests[1]));
    assert_eq!(
        fixture.used(event).await?.0,
        vec!["request:900002".to_owned()]
    );
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_the_session_opens_a_returned_request_by_id_marks_it_used()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let event = text_search(&fixture, "needle-x", &[900_003]).await?;
    fixture
        .tool_use(2, "mcp__mike__get_request", json!({"id": 900_003}))
        .await?;

    mark(&fixture).await?;

    assert_eq!(
        fixture.used(event).await?.0,
        vec!["request:900003".to_owned()]
    );
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_a_turn_read_shares_a_number_with_a_returned_request_marks_nothing()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let event = text_search(&fixture, "needle-x", &[900_004]).await?;
    fixture
        .tool_use(2, TURN_TOOL, json!({"id": 900_004}))
        .await?;

    mark(&fixture).await?;

    assert!(fixture.used(event).await?.0.is_empty());
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_the_session_opens_a_request_the_search_did_not_return_marks_nothing()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let event = text_search(&fixture, "needle-x", &[900_005]).await?;
    fixture
        .tool_use(2, OPEN_REQUEST_TOOL, json!({"request_id": 900_006}))
        .await?;

    mark(&fixture).await?;

    assert!(fixture.used(event).await?.0.is_empty());
    fixture.cleanup().await
}
