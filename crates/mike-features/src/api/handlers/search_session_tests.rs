use std::time::Duration;

use serde_json::json;

use super::search_session_test_support::*;
use super::*;

const NEEDLE: &str = "needle-once-7f3a";

fn windows_of(requests: usize, budget: Duration) -> ScanLimits {
    ScanLimits {
        body_window_requests: requests,
        message_window_requests: requests,
        budget,
    }
}

#[tokio::test]
async fn message_scan_text_resent_in_later_requests_is_reported_once_at_first_request()
-> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    let message = user_text(&format!("remember {NEEDLE} please"));
    let history = format!("history {NEEDLE}");
    let first = fx.add_request(&history).await?;
    let message_id = fx.add_message(first, 0, &message).await?;
    for _ in 0..3 {
        let later = fx.add_request(&history).await?;
        fx.add_delta(later, 0, message_id).await?;
    }

    let answer = fx.find(json!({"needle": NEEDLE})).await?;

    assert_eq!(answer["full_match_count"], json!(1), "{answer}");
    assert_eq!(match_request_ids(&answer), vec![first as u64], "{answer}");
    assert_eq!(answer["matches"][0]["message_ord"], json!(0));
    assert_eq!(answer["positions"], json!("message"));
    assert_eq!(answer["scan_complete"], json!(true));
    assert_eq!(answer["requests_scanned"], json!(4));
    assert_eq!(answer["requests_total"], json!(4));
    assert_eq!(answer["has_more"], json!(false));
    fx.finish().await
}

#[tokio::test]
async fn whole_bodies_text_resent_in_later_requests_is_reported_once_per_request()
-> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    let history = format!("history {NEEDLE}");
    for _ in 0..4 {
        fx.add_request(&history).await?;
    }

    let answer = fx
        .find(json!({"needle": NEEDLE, "whole_bodies": true}))
        .await?;

    assert_eq!(answer["full_match_count"], json!(4), "{answer}");
    assert_eq!(match_request_ids(&answer).len(), 4);
    assert_eq!(answer["positions"], json!("body"));
    assert!(answer["matches"][0].get("message_ord").is_none());
    fx.finish().await
}

#[tokio::test]
async fn message_scan_message_added_twice_is_reported_at_its_earliest_request() -> anyhow::Result<()>
{
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    let first = fx.add_request("{}").await?;
    let second = fx.add_request("{}").await?;
    let message_id = fx
        .add_message(second, 3, &user_text(&format!("late {NEEDLE}")))
        .await?;
    fx.add_delta(first, 1, message_id).await?;

    let answer = fx.find(json!({"needle": NEEDLE})).await?;

    assert_eq!(match_request_ids(&answer), vec![first as u64], "{answer}");
    assert_eq!(answer["matches"][0]["message_ord"], json!(1));
    fx.finish().await
}

#[tokio::test]
async fn message_scan_needle_with_pattern_characters_is_matched_literally() -> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    let request = fx.add_request("{}").await?;
    fx.add_message(request, 0, &user_text("price 100% done_ok"))
        .await?;
    fx.add_message(request, 1, &user_text("price 100x doneXok"))
        .await?;

    let answer = fx.find(json!({"needle": "100% done_ok"})).await?;

    assert_eq!(answer["full_match_count"], json!(1), "{answer}");
    assert_eq!(answer["matches"][0]["message_ord"], json!(0));
    fx.finish().await
}

#[tokio::test]
async fn message_scan_case_insensitive_matches_ascii_fold() -> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    let request = fx.add_request("{}").await?;
    fx.add_message(request, 0, &user_text("Shout NEEDLE-Once-7F3A loud"))
        .await?;

    let sensitive = fx.find(json!({"needle": NEEDLE})).await?;
    let folded = fx
        .find(json!({"needle": NEEDLE, "case_sensitive": false}))
        .await?;

    assert_eq!(sensitive["full_match_count"], json!(0), "{sensitive}");
    assert_eq!(folded["full_match_count"], json!(1), "{folded}");
    fx.finish().await
}

#[tokio::test]
async fn session_scan_needle_only_in_last_window_is_returned_in_one_call() -> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    for _ in 0..4 {
        fx.add_request("{}").await?;
    }
    let last = fx.add_request("{}").await?;
    fx.add_message(last, 0, &user_text(NEEDLE)).await?;

    let answer = fx
        .find_limited(NEEDLE, None, windows_of(2, Duration::from_secs(60)))
        .await?;

    assert_eq!(match_request_ids(&answer), vec![last as u64], "{answer}");
    assert_eq!(answer["scan_complete"], json!(true));
    assert_eq!(answer["full_match_count"], json!(1));
    assert_eq!(answer["requests_scanned"], json!(5));
    assert_eq!(answer["requests_total"], json!(5));
    assert_eq!(answer["has_more"], json!(false));
    assert!(answer.get("next_cursor").is_none());
    fx.finish().await
}

#[tokio::test]
async fn session_scan_time_limit_reached_reports_incomplete_with_cursor_and_resumes()
-> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    let mut ids = Vec::new();
    for _ in 0..5 {
        ids.push(fx.add_request("{}").await?);
    }
    fx.add_message(ids[4], 0, &user_text(NEEDLE)).await?;

    let first = fx
        .find_limited(NEEDLE, None, windows_of(2, Duration::ZERO))
        .await?;

    assert_eq!(first["scan_complete"], json!(false), "{first}");
    assert_eq!(first["has_more"], json!(true));
    assert_eq!(first["full_match_count"], json!(null));
    assert_eq!(first["next_cursor"], json!(ids[1]));
    assert_eq!(first["requests_scanned"], json!(2));
    assert_eq!(first["requests_total"], json!(5));
    assert!(match_request_ids(&first).is_empty());

    let resumed = fx
        .find_limited(NEEDLE, Some(ids[1] as u64), windows_of(10, Duration::ZERO))
        .await?;

    assert_eq!(resumed["scan_complete"], json!(true), "{resumed}");
    assert_eq!(match_request_ids(&resumed), vec![ids[4] as u64]);
    assert_eq!(resumed["requests_scanned"], json!(5));
    fx.finish().await
}

#[tokio::test]
async fn session_scan_match_count_continues_past_max_matches() -> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    let request = fx.add_request("{}").await?;
    for ord in 0..3 {
        fx.add_message(request, ord, &user_text(NEEDLE)).await?;
    }

    let answer = fx.find(json!({"needle": NEEDLE, "max_matches": 1})).await?;

    assert_eq!(answer["full_match_count"], json!(3), "{answer}");
    assert_eq!(answer["matches"].as_array().map(Vec::len), Some(1));
    assert_eq!(answer["matches_truncated"], json!(true));
    fx.finish().await
}

#[tokio::test]
async fn session_scan_session_without_requests_is_complete_and_empty() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };

    let answer = fx.find(json!({"needle": NEEDLE})).await?;

    assert_eq!(answer["scan_complete"], json!(true), "{answer}");
    assert_eq!(answer["full_match_count"], json!(0));
    assert_eq!(answer["requests_total"], json!(0));
    fx.finish().await
}

#[tokio::test]
async fn session_scan_unknown_session_is_not_found() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };

    let outcome = run_find(
        &fx.state,
        json!({"needle": NEEDLE, "session_id": uuid::Uuid::new_v4().to_string()}),
    )
    .await;

    let error = outcome
        .err()
        .ok_or_else(|| anyhow::anyhow!("an unknown session produced an answer"))?;
    assert!(error.to_string().contains("404"), "{error}");
    fx.finish().await
}

#[tokio::test]
async fn unscoped_search_without_source_reads_response_bodies() -> anyhow::Result<()> {
    let Some(mut fx) = fixture().await? else {
        return Ok(());
    };
    let request = fx.add_request("{}").await?;
    fx.add_response(request, &format!("assistant said {NEEDLE}"))
        .await?;

    let answer = run_find(&fx.state, json!({"needle": NEEDLE})).await?;

    assert_eq!(answer["full_match_count"], json!(1), "{answer}");
    assert_eq!(answer["matches"][0]["source"], json!("response"));
    assert_eq!(answer["matches"][0]["request_id"], json!(request));
    fx.finish().await
}

#[tokio::test]
async fn unscoped_search_explicit_request_source_is_still_rejected() -> anyhow::Result<()> {
    let Some(fx) = fixture().await? else {
        return Ok(());
    };

    let outcome = run_find(&fx.state, json!({"needle": NEEDLE, "source": "request"})).await;

    let error = outcome
        .err()
        .ok_or_else(|| anyhow::anyhow!("unscoped source=request produced an answer"))?;
    let text = error.to_string();
    assert!(text.contains("400"), "{text}");
    assert!(text.contains("unscoped search is response-only"), "{text}");
    fx.finish().await
}
