use serde_json::json;

use super::fixture_tests::{Fixture, SEARCH_TOOL};

#[tokio::test]
async fn attribute_callers_gives_an_unattributed_search_the_session_and_request_of_its_tool_call()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(2).await?;
    fixture
        .tool_use(
            1,
            SEARCH_TOOL,
            json!({"project": "p", "query": "disk full"}),
        )
        .await?;
    let event = fixture.record(None, "disk full", &["lesson:a"]).await?;

    let attributed = fixture.handle.attribute_search_callers().await?;

    assert_eq!(attributed, 1);
    let session = fixture.session_row().await?;
    assert_eq!(
        fixture.event(event).await?,
        (Some(session), Some(fixture.requests[1]))
    );
    fixture.cleanup().await
}

#[tokio::test]
async fn attribute_callers_reaches_a_matched_search_behind_a_full_batch_of_unmatched_ones()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(2).await?;
    for n in 0..3 {
        fixture
            .record(None, &format!("eval probe {n}"), &["lesson:a"])
            .await?;
    }
    fixture
        .tool_use(
            1,
            SEARCH_TOOL,
            json!({"project": "p", "query": "disk full"}),
        )
        .await?;
    let event = fixture.record(None, "disk full", &["lesson:a"]).await?;

    let attributed = fixture
        .handle
        .attribute_search_callers_within(1, 86_400.0)
        .await?;

    assert_eq!(attributed, 1);
    assert_eq!(fixture.event(event).await?.1, Some(fixture.requests[1]));
    fixture.cleanup().await
}

#[tokio::test]
async fn attribute_callers_leaves_a_search_with_no_matching_tool_call_unattributed()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(2).await?;
    fixture
        .tool_use(
            1,
            SEARCH_TOOL,
            json!({"project": "p", "query": "another query"}),
        )
        .await?;
    let event = fixture.record(None, "disk full", &["lesson:a"]).await?;

    fixture.handle.attribute_search_callers().await?;

    assert_eq!(fixture.event(event).await?, (None, None));
    fixture.cleanup().await
}

#[tokio::test]
async fn attribute_callers_does_not_touch_a_search_that_already_has_a_session() -> anyhow::Result<()>
{
    let fixture = Fixture::boot(2).await?;
    fixture
        .tool_use(
            1,
            SEARCH_TOOL,
            json!({"project": "p", "query": "disk full"}),
        )
        .await?;
    let event = fixture
        .record(Some(fixture.session), "disk full", &["lesson:a"])
        .await?;
    let before = fixture.event(event).await?;

    let attributed = fixture.handle.attribute_search_callers().await?;

    assert_eq!(attributed, 0);
    assert_eq!(fixture.event(event).await?, before);
    fixture.cleanup().await
}
