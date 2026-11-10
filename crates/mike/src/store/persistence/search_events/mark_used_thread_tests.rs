use serde_json::json;

use super::fixture_tests::{Fixture, SEARCH_TOOL};
use super::mark_used::USED_BATCH;

async fn search_in_request(fixture: &Fixture, query: &str, refs: &[&str]) -> anyhow::Result<i64> {
    fixture
        .tool_use(1, SEARCH_TOOL, json!({"project": "p", "query": query}))
        .await?;
    fixture.record(Some(fixture.session), query, refs).await
}

#[tokio::test]
async fn mark_used_when_the_session_gets_a_returned_work_thread_marks_it_used() -> anyhow::Result<()>
{
    for argument in ["slug", "thread"] {
        let fixture = Fixture::boot(6).await?;
        let event = search_in_request(
            &fixture,
            "motion",
            &["work_thread:frontend-motion", "work_thread:other"],
        )
        .await?;
        fixture
            .tool_use(
                2,
                "mcp__mike__work_thread_get",
                json!({argument: "frontend-motion"}),
            )
            .await?;

        fixture
            .handle
            .mark_used_search_references(USED_BATCH)
            .await?;

        assert_eq!(
            fixture.used(event).await?.0,
            vec!["work_thread:frontend-motion".to_owned()],
            "{argument}"
        );
        fixture.cleanup().await?;
    }
    Ok(())
}

#[tokio::test]
async fn mark_used_when_the_final_reply_names_a_work_thread_slug_marks_it_only_when_bounded()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let named = search_in_request(&fixture, "motion", &["work_thread:frontend-motion"]).await?;
    let longer = search_in_request(&fixture, "motion v2", &["work_thread:frontend"]).await?;
    fixture
        .final_reply(
            2,
            "The frontend-motion thread says the easing is done; frontend-v2 is separate.",
        )
        .await?;

    fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;

    assert_eq!(
        fixture.used(named).await?.0,
        vec!["work_thread:frontend-motion".to_owned()]
    );
    assert!(fixture.used(longer).await?.0.is_empty());
    fixture.cleanup().await
}

#[tokio::test]
async fn mark_used_when_a_work_thread_is_only_read_by_another_slug_marks_nothing()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(6).await?;
    let event = search_in_request(&fixture, "motion", &["work_thread:frontend-motion"]).await?;
    fixture
        .tool_use(
            2,
            "mcp__mike__work_thread_get",
            json!({"slug": "frontend-motion-v2"}),
        )
        .await?;

    fixture
        .handle
        .mark_used_search_references(USED_BATCH)
        .await?;

    assert!(fixture.used(event).await?.0.is_empty());
    fixture.cleanup().await
}
