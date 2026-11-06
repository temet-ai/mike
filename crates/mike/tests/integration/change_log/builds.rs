use serde_json::{Value, json};

use super::Stack;

fn build(sha: &str, batch_default: i64) -> Value {
    json!({
        "sha": sha,
        "sha_short": &sha[..7.min(sha.len())],
        "dirty": false,
        "built_at": "2026-09-24T12:00:00Z",
        "defaults": {
            "settings": {"dossier.batch_sessions": batch_default},
            "toggles": {"dossier_builder": {"enabled": false}}
        }
    })
}

async fn build_rows(stack: &Stack) -> anyhow::Result<Vec<Value>> {
    let page = stack
        .get("/api/config/change-log?subject_kind=build&limit=500")
        .await?;
    Ok(page["changes"].as_array().cloned().unwrap_or_default())
}

#[tokio::test]
async fn builds_write_a_row_only_when_sha_or_defaults_change() -> anyhow::Result<()> {
    let Some(stack) = Stack::start().await? else {
        return Ok(());
    };
    let persist = stack
        .state
        .persist()
        .ok_or_else(|| anyhow::anyhow!("persistence must be on"))?;
    sqlx::query("DELETE FROM config.change_log WHERE subject_kind = 'build'")
        .execute(&stack.raw)
        .await?;
    let first = persist
        .record_build_value(&build("aaaaaaaaaa", 25), "aaaaaaaaaa")
        .await?;
    assert!(first.is_some(), "first build writes a row");
    let second = persist
        .record_build_value(&build("bbbbbbbbbb", 25), "bbbbbbbbbb")
        .await?;
    assert!(second.is_some(), "a different build writes a row");
    let repeat = persist
        .record_build_value(&build("bbbbbbbbbb", 25), "bbbbbbbbbb")
        .await?;
    assert!(repeat.is_none(), "a repeat boot writes nothing");
    let defaults_only = persist
        .record_build_value(&build("bbbbbbbbbb", 30), "bbbbbbbbbb")
        .await?;
    assert!(
        defaults_only.is_some(),
        "a changed code default writes a row"
    );
    let rows = build_rows(&stack).await?;
    assert_eq!(rows.len(), 3, "rows: {rows:?}");
    assert_eq!(
        rows[0]["detail"]["defaults_changed"],
        json!(["setting:dossier.batch_sessions"])
    );
    assert_eq!(rows[0]["via"], "startup");
    stack.finish().await
}
