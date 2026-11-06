use serde_json::json;

use super::{SPA, api};
use crate::turn_facts::fixture;
use crate::turn_facts::resolution::message;

#[tokio::test]
async fn vocabulary_counts_per_axis_match_the_design() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let counts: Vec<(String, i64)> =
        sqlx::query_as("SELECT axis, count(*) FROM eval.axis_labels GROUP BY axis ORDER BY axis")
            .fetch_all(f.handle.pool())
            .await?;
    let get = |axis: &str| {
        counts
            .iter()
            .find(|(a, _)| a == axis)
            .map_or(0, |(_, n)| *n)
    };
    assert_eq!(get("reaction"), 5);
    assert_eq!(get("delivery"), 6);
    assert_eq!(get("session_end"), 5);
    assert_eq!(get("failure_tag"), 24);
    assert_eq!(get("steer_valid"), 3);
    for axis in [
        "fact_own_prompt_corrects_prior",
        "fact_assistant_self_correction",
        "fact_irreversible_action",
    ] {
        assert_eq!(get(axis), 2, "{axis}");
    }
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_gold_post_without_origin_is_refused_and_writes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = message(&f, 9100, 0, "please add a retry", Some(100)).await?;
    let api = api(&f).await?;
    let body = json!({"subject": {"kind": "turn", "turn_id": turn}, "labels": [{"axis": "reaction", "label": "accepted"}]});
    let (status, _) = api.post("/api/eval/labels", body, None).await?;
    assert_eq!(status, 403);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM eval.labels")
        .fetch_one(f.handle.pool())
        .await?;
    assert_eq!(rows, 0);
    api.stop();
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn turn_split_follows_the_hash_bucket_for_twenty_turns() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let api = api(&f).await?;
    for i in 0..20 {
        let turn = message(&f, 9100 + i, i * 10, "please add a retry", Some(i * 10 + 5)).await?;
        let body = json!({"subject": {"kind": "turn", "turn_id": turn}, "labels": [{"axis": "reaction", "label": "accepted"}]});
        let (status, created) = api.post("/api/eval/labels", body, Some(SPA)).await?;
        assert_eq!(status, 201, "{created}");
        let expected: String = sqlx::query_scalar(
            "SELECT CASE WHEN (((hashtextextended($1::text, 0) % 100) + 100) % 100) < 30 THEN 'holdout' ELSE 'dev' END",
        )
        .bind(turn)
        .fetch_one(f.handle.pool())
        .await?;
        assert_eq!(
            created["labels"][0]["split"],
            json!(expected),
            "turn {turn}"
        );
    }
    api.stop();
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn withdraw_removes_label_from_current_view_and_as_of_later() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = message(&f, 9100, 0, "please add a retry", Some(100)).await?;
    let api = api(&f).await?;
    let body = json!({"subject": {"kind": "turn", "turn_id": turn}, "labels": [{"axis": "reaction", "label": "corrected"}]});
    let (_, created) = api.post("/api/eval/labels", body, Some(SPA)).await?;
    let id = created["labels"][0]["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("no id: {created}"))?;
    let between = crate::seed::database_now(f.handle.pool()).await?;
    let (status, _) = api
        .post(
            &format!("/api/eval/labels/{id}/withdraw"),
            json!({}),
            Some(SPA),
        )
        .await?;
    assert_eq!(status, 201);
    let current: i64 =
        sqlx::query_scalar("SELECT count(*) FROM eval.labels_current_v WHERE subject_id = $1")
            .bind(turn)
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(current, 0);
    let earlier: i64 =
        sqlx::query_scalar("SELECT count(*) FROM eval.labels_as_of($1) WHERE subject_id = $2")
            .bind(between)
            .bind(turn)
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(
        earlier, 1,
        "as of before the withdrawal the label was current"
    );
    api.stop();
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn relay_refuses_an_unfound_quote_and_a_switched_off_setting() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = message(&f, 9100, 0, "that turn was fine, it was done", Some(100)).await?;
    let session: uuid::Uuid =
        sqlx::query_scalar("SELECT session_uuid FROM sessions.sessions WHERE id = $1")
            .bind(f.session_id)
            .fetch_one(f.handle.pool())
            .await?;
    let api = api(&f).await?;
    let relay = |quote: &str| {
        json!({"subject_kind": "turn", "turn_id": turn, "axis": "reaction", "label": "accepted",
               "operator_quote": quote, "quote_session_id": session})
    };
    let (status, _) = api
        .post(
            "/api/eval/labels/relay",
            relay("words nobody ever typed"),
            None,
        )
        .await?;
    assert_eq!(status, 422);
    let (status, created) = api
        .post("/api/eval/labels/relay", relay("it was done"), None)
        .await?;
    assert_eq!(status, 201, "{created}");
    assert_eq!(created["label"]["tier"], "relayed");
    f.handle
        .upsert_setting("eval.relay_labels_enabled", &json!(false))
        .await?;
    let (status, refused) = api
        .post("/api/eval/labels/relay", relay("it was done"), None)
        .await?;
    assert_eq!(status, 403);
    assert!(
        refused["error"]
            .as_str()
            .is_some_and(|e| e.contains("eval.relay_labels_enabled"))
    );
    api.stop();
    f.db.cleanup().await?;
    Ok(())
}
