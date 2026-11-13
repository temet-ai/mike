use serde_json::{Value, json};

use crate::api::handlers::search_session::search_session_test_support::*;

const NEEDLE: &str = "needle-logged-5c1e";

async fn logged_rows(fx: &Fixture) -> anyhow::Result<Vec<(String, String, Option<i64>, Value)>> {
    fx.state.tracker.close();
    fx.state.tracker.wait().await;
    fx.state.tracker.reopen();
    Ok(sqlx::query_as(
        "SELECT tool, query, session_id, returned FROM memory.search_events ORDER BY id",
    )
    .fetch_all(fx.pool()?)
    .await?)
}

#[tokio::test]
async fn text_search_with_a_caller_logs_one_row_with_the_matching_requests() -> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    for _ in 0..2 {
        fx.add_request(&format!("history {NEEDLE}")).await?;
    }

    let answer = run_find(
        &fx.state,
        json!({
            "needle": NEEDLE,
            "session_id": fx.session_uuid,
            "whole_bodies": true,
            "caller_session_id": fx.session_uuid,
        }),
    )
    .await?;

    let requests = match_request_ids(&answer);
    assert_eq!(requests.len(), 2, "{answer}");
    let rows = logged_rows(&fx).await?;
    assert_eq!(rows.len(), 1, "{rows:?}");
    let (tool, query, session, returned) = &rows[0];
    assert_eq!((tool.as_str(), query.as_str()), ("find_text", NEEDLE));
    assert!(session.is_some());
    let expected: Vec<Value> = requests
        .iter()
        .map(|id| json!({"reference": format!("request:{id}")}))
        .collect();
    assert_eq!(returned, &Value::Array(expected));
    fx.finish().await
}

#[tokio::test]
async fn text_search_without_a_caller_logs_no_row() -> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    fx.add_request(&format!("history {NEEDLE}")).await?;

    run_find(
        &fx.state,
        json!({"needle": NEEDLE, "session_id": fx.session_uuid, "whole_bodies": true}),
    )
    .await?;

    assert!(logged_rows(&fx).await?.is_empty());
    fx.finish().await
}

#[tokio::test]
async fn unscoped_text_search_with_no_match_logs_a_row_that_returned_nothing() -> anyhow::Result<()>
{
    let Some(fx) = fixture().await? else {
        return Ok(());
    };

    run_find(
        &fx.state,
        json!({"needle": NEEDLE, "caller_session_id": fx.session_uuid}),
    )
    .await?;

    let rows = logged_rows(&fx).await?;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].3, json!([]));
    fx.finish().await
}
