use crate::anthropic_fixtures;
use crate::common;

use super::support::{poll_detail_for_complete, post_non_streaming_and_get_id, start_json_mock};
use anthropic_fixtures::MockResponse;

#[tokio::test]
async fn non_streaming_response_with_trigger_text_produces_diagnostics() -> anyhow::Result<()> {
    let trigger = "This is a pragmatic and robust implementation";
    let mock_server = start_json_mock(
        MockResponse::new("msg_non_streaming")
            .text(trigger)
            .usage(5, 5)
            .build(),
    )
    .await;

    let (proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let id = post_non_streaming_and_get_id(&client, &proxy_url, &inspect_url).await?;

    let detail = poll_detail_for_complete(&client, &inspect_url, id, true).await?;

    assert_eq!(
        detail["status"], "Complete",
        "non-streaming response must reach Complete: {detail}"
    );

    let diagnostics = detail["diagnostics"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("diagnostics must be a JSON array: {detail}"))?;
    assert!(
        !diagnostics.is_empty(),
        "non-streaming analyzer hook should have produced at least one diagnostic: {detail}"
    );

    let has_js = diagnostics
        .iter()
        .any(|d| d["category"].as_str() == Some("justifying_shortcuts"));
    let has_overselling = diagnostics
        .iter()
        .any(|d| d["category"].as_str() == Some("overselling"));
    assert!(
        has_js,
        "non-streaming response should fire JustifyingShortcuts: {diagnostics:?}"
    );
    assert!(
        has_overselling,
        "non-streaming response should fire Overselling: {diagnostics:?}"
    );

    Ok(())
}

#[tokio::test]
async fn non_streaming_response_with_no_trigger_text_produces_no_diagnostics() -> anyhow::Result<()>
{
    let mock_server = start_json_mock(
        MockResponse::new("msg_clean")
            .text("Hello.")
            .usage(5, 1)
            .build(),
    )
    .await;

    let (proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let id = post_non_streaming_and_get_id(&client, &proxy_url, &inspect_url).await?;
    let detail = poll_detail_for_complete(&client, &inspect_url, id, false).await?;

    assert_eq!(detail["status"], "Complete");
    let diagnostics = detail["diagnostics"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("diagnostics must still be a JSON array: {detail}"))?;
    assert!(
        diagnostics.is_empty(),
        "clean text must produce zero diagnostics, got {diagnostics:?}"
    );
    Ok(())
}
