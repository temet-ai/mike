use serde_json::json;

use super::{SPA_ORIGIN, Stack, non_observe};

const KEY: &str = "dossier.batch_sessions";

#[tokio::test]
async fn baseline_has_at_most_one_observe_row_per_key_and_covers_every_row() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let duplicates: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (
             SELECT subject_kind, subject_key
               FROM config.change_log
              WHERE op = 'observe' AND subject_kind <> 'build'
              GROUP BY subject_kind, subject_key
             HAVING count(*) > 1) d",
    )
    .fetch_one(&stack.raw)
    .await?;
    assert_eq!(duplicates, 0, "one baseline row per existing row");
    let uncovered: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (
             SELECT 'toggle' AS kind, key FROM config.feature_toggles
             UNION ALL SELECT 'setting', key FROM config.settings
             UNION ALL SELECT 'steerer', id::text FROM steering.steerers) t
          WHERE NOT EXISTS (
              SELECT 1 FROM config.change_log c
               WHERE c.subject_kind = t.kind AND c.subject_key = t.key)",
    )
    .fetch_one(&stack.raw)
    .await?;
    assert_eq!(
        uncovered, 0,
        "every existing row has a baseline or insert row"
    );
    stack.finish().await
}

#[tokio::test]
async fn patch_records_api_then_spa_and_an_identical_patch_records_nothing() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let route = format!("/api/settings/{KEY}");
    stack.patch(&route, json!({"value": 7}), None).await?;
    stack
        .patch(&route, json!({"value": 9}), Some(SPA_ORIGIN))
        .await?;
    stack
        .patch(&route, json!({"value": 9}), Some(SPA_ORIGIN))
        .await?;
    let rows = stack.rows_for("setting", KEY).await?;
    let changes = non_observe(&rows);
    assert_eq!(changes.len(), 2, "rows: {rows:?}");
    assert_eq!(changes[0]["via"], "spa");
    assert_eq!(changes[0]["old_value"], json!(7));
    assert_eq!(changes[0]["new_value"], json!(9));
    assert_eq!(changes[1]["via"], "api");
    assert!(
        changes[1]["request_id"].is_string(),
        "x-request-id is recorded"
    );
    assert_eq!(changes[1]["detail"]["applies"], "next_cycle");
    stack.finish().await
}

#[tokio::test]
async fn toggle_patch_records_enabled_and_operator_set() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    stack
        .patch(
            "/api/settings/features/dossier_builder",
            json!({"enabled": true}),
            None,
        )
        .await?;
    let rows = stack.rows_for("toggle", "dossier_builder").await?;
    let changes = non_observe(&rows);
    assert_eq!(changes.len(), 1, "rows: {rows:?}");
    assert_eq!(
        changes[0]["new_value"],
        json!({"enabled": true, "operator_set": true})
    );
    stack.finish().await
}

#[tokio::test]
async fn psql_update_records_sql_and_delete_records_delete() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    sqlx::query("INSERT INTO config.settings (key, value) VALUES ($1, '3'::jsonb)")
        .bind(KEY)
        .execute(&stack.raw)
        .await?;
    sqlx::query("UPDATE config.settings SET value = '4'::jsonb WHERE key = $1")
        .bind(KEY)
        .execute(&stack.raw)
        .await?;
    sqlx::query("DELETE FROM config.settings WHERE key = $1")
        .bind(KEY)
        .execute(&stack.raw)
        .await?;
    let rows = stack.rows_for("setting", KEY).await?;
    let ops: Vec<(String, String)> = non_observe(&rows)
        .iter()
        .map(|row| {
            (
                row["op"].as_str().unwrap_or_default().to_owned(),
                row["via"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        ops,
        vec![
            ("delete".to_owned(), "sql".to_owned()),
            ("update".to_owned(), "sql".to_owned()),
            ("insert".to_owned(), "sql".to_owned()),
        ]
    );
    stack.finish().await
}

#[tokio::test]
async fn steerer_soft_delete_records_both_prompts_whole() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let prompt = "judge carefully. ".repeat(1_200);
    let created = reqwest::Client::new()
        .post(format!("{}/api/steerers", stack.base))
        .json(&json!({
            "name": "change-log-probe",
            "display_name": "Change log probe",
            "authority": "command",
            "mode": "drive",
            "model": "openai/gpt-4o-mini",
            "prompt": prompt,
        }))
        .send()
        .await?;
    anyhow::ensure!(
        created.status().is_success(),
        "create: {}",
        created.status()
    );
    let body: serde_json::Value = created.json().await?;
    let id = body["steerer"]["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("no steerer id in {body}"))?;
    let deleted = reqwest::Client::new()
        .delete(format!("{}/api/steerers/{id}", stack.base))
        .send()
        .await?;
    anyhow::ensure!(
        deleted.status().is_success(),
        "delete: {}",
        deleted.status()
    );
    let rows = stack.rows_for("steerer", &id.to_string()).await?;
    let soft = rows
        .iter()
        .find(|row| row["op"] == json!("soft_delete"))
        .ok_or_else(|| anyhow::anyhow!("no soft_delete row in {rows:?}"))?;
    assert_eq!(soft["old_value"]["prompt"], json!(prompt));
    assert_eq!(soft["new_value"]["prompt"], json!(prompt));
    assert_eq!(soft["via"], "api");
    stack.finish().await
}

#[tokio::test]
async fn secret_key_values_are_redacted_everywhere() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let secret = "sk-change-log-secret-9f3a";
    sqlx::query("INSERT INTO config.settings (key, value) VALUES ('integrations.slack.token', to_jsonb($1::text))")
        .bind(secret)
        .execute(&stack.raw)
        .await?;
    sqlx::query("UPDATE config.settings SET value = to_jsonb($1::text) WHERE key = 'integrations.slack.token'")
        .bind(format!("{secret}-2"))
        .execute(&stack.raw)
        .await?;
    let rows = stack
        .rows_for("setting", "integrations.slack.token")
        .await?;
    let update = rows
        .iter()
        .find(|row| row["op"] == json!("update"))
        .ok_or_else(|| anyhow::anyhow!("no update row in {rows:?}"))?;
    assert_eq!(update["old_value"], json!({"redacted": true}));
    assert_eq!(update["new_value"], json!({"redacted": true}));
    assert_eq!(update["redacted"], json!(true));
    let leaks: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM config.change_log c
          WHERE to_jsonb(c)::text LIKE '%' || $1 || '%'",
    )
    .bind(secret)
    .fetch_one(&stack.raw)
    .await?;
    assert_eq!(leaks, 0, "the secret must appear nowhere in the table");
    stack.finish().await
}
