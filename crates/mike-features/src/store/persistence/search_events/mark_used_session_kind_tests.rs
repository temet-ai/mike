use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use serde_json::json;
use uuid::Uuid;

use super::fixture_tests::{Fixture, SEARCH_TOOL};
use super::mark_used::USED_BATCH;

const READ_TOOL: &str = "mcp__mike__read_project_knowledge";
const OPEN_SESSION_TOOL: &str = "mcp__mike__get_session_overview";

async fn search_returning_sessions(
    fixture: &Fixture,
    sessions: &[Uuid],
) -> anyhow::Result<(i64, Vec<String>)> {
    fixture
        .tool_use(
            1,
            SEARCH_TOOL,
            json!({"project": "p", "query": "overview words"}),
        )
        .await?;
    let references: Vec<String> = sessions
        .iter()
        .map(|session| format!("session:{session}"))
        .collect();
    let borrowed: Vec<&str> = references.iter().map(String::as_str).collect();
    let event = fixture
        .record(Some(fixture.session), "overview words", &borrowed)
        .await?;
    Ok((event, references))
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
async fn mark_used_when_the_session_reads_a_returned_session_overview_marks_only_that_one()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let (read, passed_over) = (Uuid::new_v4(), Uuid::new_v4());
    let (event, _) = search_returning_sessions(&fixture, &[read, passed_over]).await?;
    fixture
        .tool_use(
            2,
            READ_TOOL,
            json!({"reference": format!("session:{read}")}),
        )
        .await?;

    mark(&fixture).await?;

    assert_eq!(
        fixture.used(event).await?.0,
        vec![format!("session:{read}")]
    );
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_the_session_reads_a_returned_overview_by_its_bare_uuid_marks_it()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let found = Uuid::new_v4();
    let (event, _) = search_returning_sessions(&fixture, &[found]).await?;
    fixture
        .tool_use(2, READ_TOOL, json!({"reference": found.to_string()}))
        .await?;

    mark(&fixture).await?;

    assert_eq!(
        fixture.used(event).await?.0,
        vec![format!("session:{found}")]
    );
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_the_session_opens_the_conversation_of_a_returned_overview_marks_it()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let found = Uuid::new_v4();
    let (event, _) = search_returning_sessions(&fixture, &[found]).await?;
    fixture
        .tool_use(
            2,
            OPEN_SESSION_TOOL,
            json!({"session_id": found.to_string()}),
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
async fn mark_used_when_the_session_reads_an_overview_the_search_did_not_return_marks_nothing()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let (event, _) = search_returning_sessions(&fixture, &[Uuid::new_v4()]).await?;
    fixture
        .tool_use(
            2,
            READ_TOOL,
            json!({"reference": format!("session:{}", Uuid::new_v4())}),
        )
        .await?;

    mark(&fixture).await?;

    assert!(fixture.used(event).await?.0.is_empty());
    fixture.cleanup().await
}
