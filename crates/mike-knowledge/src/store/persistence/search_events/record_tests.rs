use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use serde_json::{Value, json};
use uuid::Uuid;

use super::NewSearchEvent;
use super::fixture_tests::{Fixture, SEARCH_TOOL};

#[tokio::test]
async fn record_search_event_with_known_session_stores_the_calling_request() -> anyhow::Result<()> {
    let fixture = Fixture::boot(3).await?;
    fixture
        .tool_use(
            1,
            SEARCH_TOOL,
            json!({"project": "p", "query": "cache rules"}),
        )
        .await?;

    let id = fixture
        .record(
            Some(fixture.session),
            "cache rules",
            &["lesson:a", "turn:7"],
        )
        .await?;

    let (session, request) = fixture.event(id).await?;
    assert_eq!(session, Some(fixture.session_row().await?));
    assert_eq!(request, Some(fixture.requests[1]));
    let returned: Value =
        sqlx::query_scalar("SELECT returned FROM memory.search_events WHERE id = $1")
            .bind(id)
            .fetch_one(fixture.handle.pool())
            .await?;
    assert_eq!(returned[0]["reference"], "lesson:a");
    assert_eq!(returned[1]["reference"], "turn:7");
    fixture.cleanup().await
}

#[tokio::test]
async fn record_search_event_without_a_captured_tool_use_falls_back_to_the_newest_request()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(3).await?;

    let id = fixture
        .record(Some(fixture.session), "unseen call", &["lesson:a"])
        .await?;

    let (session, request) = fixture.event(id).await?;
    assert_eq!(session, Some(fixture.session_row().await?));
    assert_eq!(request, fixture.requests.last().copied());
    fixture.cleanup().await
}

#[tokio::test]
async fn record_search_event_with_an_unknown_session_stores_a_null_session() -> anyhow::Result<()> {
    let fixture = Fixture::boot(1).await?;

    let id = fixture
        .record(Some(Uuid::new_v4()), "who am i", &["lesson:a"])
        .await?;

    assert_eq!(fixture.event(id).await?, (None, None));
    fixture.cleanup().await
}

#[tokio::test]
async fn record_search_event_without_a_session_stores_a_null_session_and_keeps_the_rest()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(1).await?;

    let id = fixture.record(None, "anonymous", &[]).await?;

    assert_eq!(fixture.event(id).await?, (None, None));
    let (tool, state): (String, String) =
        sqlx::query_as("SELECT tool, rerank_state FROM memory.search_events WHERE id = $1")
            .bind(id)
            .fetch_one(fixture.handle.pool())
            .await?;
    assert_eq!(
        (tool.as_str(), state.as_str()),
        ("search_project_knowledge", "off")
    );
    fixture.cleanup().await
}

#[tokio::test]
async fn record_search_event_with_an_unknown_tool_is_refused_by_the_table() -> anyhow::Result<()> {
    let fixture = Fixture::boot(1).await?;
    let returned = json!([]);

    let outcome = fixture
        .handle
        .knowledge()
        .record_search_event(&NewSearchEvent {
            session: None,
            tool: "search_everything",
            query: "q",
            kind_filter: None,
            returned: &returned,
            rerank_state: "off",
        })
        .await;

    assert!(outcome.is_err());
    fixture.cleanup().await
}
