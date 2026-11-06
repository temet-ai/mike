use serde_json::{Value, json};

use super::Stack;

fn entry<'a>(snapshot: &'a Value, kind: &str, key: &str) -> Option<&'a Value> {
    snapshot["entries"]
        .as_array()?
        .iter()
        .find(|e| e["subject_kind"] == json!(kind) && e["subject_key"] == json!(key))
}

#[tokio::test]
async fn snapshot_between_two_changes_returns_the_first_value() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let route = "/api/settings/dossier.batch_sessions";
    stack.patch(route, json!({"value": 11}), None).await?;
    let between: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&stack.raw)
        .await?;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    stack.patch(route, json!({"value": 12}), None).await?;
    let at = between.to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    let snapshot = stack
        .get(&format!("/api/config/snapshot?at={}", urlencode(&at)))
        .await?;
    let batch = entry(&snapshot, "setting", "dossier.batch_sessions")
        .ok_or_else(|| anyhow::anyhow!("no batch_sessions entry: {snapshot}"))?;
    assert_eq!(batch["value"], json!(11));
    assert_eq!(batch["source"], "stored");
    let toggle = entry(&snapshot, "toggle", "steerer")
        .ok_or_else(|| anyhow::anyhow!("baseline toggle missing: {snapshot}"))?;
    assert_eq!(
        toggle["source"], "stored",
        "untouched keys come from the baseline"
    );
    assert!(snapshot["baseline_at"].is_string());
    stack.finish().await
}

fn urlencode(text: &str) -> String {
    text.replace('+', "%2B").replace(':', "%3A")
}
