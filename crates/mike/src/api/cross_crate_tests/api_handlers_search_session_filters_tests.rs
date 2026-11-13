use serde_json::{Value, json};

use crate::api::handlers::search_session::search_session_test_support::*;

const NEEDLE: &str = "needle-filter-5c1e";

fn reminder_message(text: &str) -> Value {
    user_text(&format!("<system-reminder>\n{text}\n</system-reminder>"))
}

fn search_call(name: &str, id: &str) -> Value {
    json!({"role": "assistant", "content": [
        {"type": "tool_use", "id": id, "name": name, "input": {"needle": NEEDLE}}
    ]})
}

fn search_result(id: &str) -> Value {
    json!({"role": "user", "content": [
        {"type": "tool_result", "tool_use_id": id, "content": format!("{{\"needle\":\"{NEEDLE}\"}}")}
    ]})
}

#[tokio::test]
async fn message_scan_match_inside_reminder_is_dropped_and_counted() -> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    let request = fx.add_request("{}").await?;
    fx.add_message(request, 0, &reminder_message(NEEDLE))
        .await?;
    fx.add_message(request, 1, &user_text(&format!("real {NEEDLE}")))
        .await?;

    let answer = fx.find(json!({"needle": NEEDLE})).await?;

    assert_eq!(answer["full_match_count"], json!(1), "{answer}");
    assert_eq!(answer["reminder_matches_skipped"], json!(1));
    assert_eq!(answer["matches"][0]["message_ord"], json!(1));
    fx.finish().await
}

#[tokio::test]
async fn message_scan_match_inside_reminder_is_kept_when_reminders_are_included()
-> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    let request = fx.add_request("{}").await?;
    fx.add_message(request, 0, &reminder_message(NEEDLE))
        .await?;
    fx.add_message(request, 1, &user_text(&format!("real {NEEDLE}")))
        .await?;

    let answer = fx
        .find(json!({"needle": NEEDLE, "include_reminders": true}))
        .await?;

    assert_eq!(answer["full_match_count"], json!(2), "{answer}");
    assert_eq!(answer["reminder_matches_skipped"], json!(0));
    fx.finish().await
}

#[tokio::test]
async fn message_scan_search_tool_call_and_its_result_are_dropped_and_counted() -> anyhow::Result<()>
{
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    let request = fx.add_request("{}").await?;
    fx.add_message(request, 0, &user_text(&format!("genuine {NEEDLE}")))
        .await?;
    fx.add_message(
        request,
        1,
        &search_call("mcp__mike__find_text", "toolu_own1"),
    )
    .await?;
    fx.add_message(request, 2, &search_result("toolu_own1"))
        .await?;

    let answer = fx.find(json!({"needle": NEEDLE})).await?;

    assert_eq!(answer["full_match_count"], json!(1), "{answer}");
    assert_eq!(answer["own_search_matches_skipped"], json!(2));
    assert_eq!(answer["matches"][0]["message_ord"], json!(0));
    fx.finish().await
}

#[tokio::test]
async fn message_scan_non_search_tool_call_with_needle_is_kept() -> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    let request = fx.add_request("{}").await?;
    fx.add_message(request, 0, &search_call("Bash", "toolu_bash1"))
        .await?;

    let answer = fx.find(json!({"needle": NEEDLE})).await?;

    assert_eq!(answer["full_match_count"], json!(1), "{answer}");
    assert_eq!(answer["own_search_matches_skipped"], json!(0));
    fx.finish().await
}

#[tokio::test]
async fn response_scan_response_of_request_with_search_call_is_dropped_and_counted()
-> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    let own = fx.add_request("{}").await?;
    fx.add_response(own, &format!("calling find_text for {NEEDLE}"))
        .await?;
    fx.add_tool_use(own, "mcp__mike__find_text", &json!({"needle": NEEDLE}))
        .await?;
    let genuine = fx.add_request("{}").await?;
    fx.add_response(genuine, &format!("the answer mentions {NEEDLE}"))
        .await?;
    fx.add_tool_use(
        genuine,
        "Bash",
        &json!({"command": format!("echo {NEEDLE}")}),
    )
    .await?;

    let answer = fx
        .find(json!({"needle": NEEDLE, "source": "response"}))
        .await?;

    assert_eq!(match_request_ids(&answer), vec![genuine as u64], "{answer}");
    assert_eq!(answer["full_match_count"], json!(1));
    assert_eq!(answer["own_search_matches_skipped"], json!(1));
    assert_eq!(answer["positions"], json!("body"));
    fx.finish().await
}

#[tokio::test]
async fn unscoped_response_scan_response_of_request_with_search_call_is_dropped_and_counted()
-> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    let own = fx.add_request("{}").await?;
    fx.add_response(own, &format!("calling for {NEEDLE}"))
        .await?;
    fx.add_tool_use(
        own,
        "mcp__mike__search_session_content",
        &json!({"q": NEEDLE}),
    )
    .await?;
    let genuine = fx.add_request("{}").await?;
    fx.add_response(genuine, &format!("plain {NEEDLE}")).await?;

    let answer = run_find(&fx.state, json!({"needle": NEEDLE})).await?;

    assert_eq!(match_request_ids(&answer), vec![genuine as u64], "{answer}");
    assert_eq!(answer["own_search_matches_skipped"], json!(1));
    fx.finish().await
}
