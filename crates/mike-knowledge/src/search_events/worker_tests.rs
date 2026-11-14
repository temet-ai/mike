use serde_json::json;

use super::worker::run_pass;
use crate::store::persistence::search_events::fixture_tests::{Fixture, SEARCH_TOOL};

#[tokio::test]
async fn run_pass_when_the_session_read_a_returned_reference_marks_the_event_used()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(5).await?;
    fixture
        .tool_use(
            1,
            SEARCH_TOOL,
            json!({"project": "p", "query": "cache rules"}),
        )
        .await?;
    let event = fixture
        .record(
            Some(fixture.session),
            "cache rules",
            &["lesson:a", "lesson:b"],
        )
        .await?;
    fixture
        .tool_use(
            2,
            "mcp__mike__read_project_knowledge",
            json!({"reference": "lesson:b"}),
        )
        .await?;

    run_pass(&fixture.handle).await;

    assert_eq!(
        fixture.used(event).await?,
        (vec!["lesson:b".to_owned()], false)
    );
    fixture.cleanup().await
}

#[tokio::test]
async fn run_pass_with_no_open_events_changes_nothing() -> anyhow::Result<()> {
    let fixture = Fixture::boot(2).await?;
    let event = fixture.record(None, "anonymous", &["lesson:a"]).await?;

    run_pass(&fixture.handle).await;

    assert_eq!(fixture.used(event).await?, (Vec::<String>::new(), false));
    fixture.cleanup().await
}
