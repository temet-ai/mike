use super::harness::start_proxy_with_config;

use serde_json::{Value, json};

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn list_empty_returns_empty_array() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/filter/rules", proxy.inspect_url))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let rules: Vec<Value> = resp.json().await?;
    assert!(rules.is_empty(), "expected empty list, got {rules:?}");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn create_rule_returns_201_with_full_dto() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let value = format!("tc-create-{}", uuid::Uuid::new_v4());
    let resp = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .header("Origin", "http://127.0.0.1:9990")
        .json(&json!({
            "kind": "xml_tag",
            "value": value,
            "action": "drop",
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 201);
    let envelope: Value = resp.json().await?;
    let dto = &envelope["rule"];
    assert_eq!(dto["kind"], "xml_tag");
    assert_eq!(dto["value"], value);
    assert_eq!(dto["action"], "drop");
    assert_eq!(dto["enabled"], true);
    assert_eq!(dto["source"], "ui");
    assert!(dto["id"].as_str().is_some());
    assert!(dto["updated_at"].as_str().is_some());
    assert_eq!(
        envelope["warnings"],
        Value::Array(vec![]),
        "expected no warnings on happy path, got: {envelope:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn create_duplicate_kind_value_permitted_with_distinct_ids() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let value = format!("tc-dup-{}", uuid::Uuid::new_v4());
    let body = json!({"kind": "xml_tag", "value": value, "action": "drop"});
    let r1 = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&body)
        .send()
        .await?;
    assert_eq!(r1.status(), 201);
    let e1: Value = r1.json().await?;
    let r2 = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&body)
        .send()
        .await?;
    assert_eq!(
        r2.status(),
        201,
        "duplicate (kind,value) is permitted post-squash — no UNIQUE constraint, no 409"
    );
    let e2: Value = r2.json().await?;
    assert_ne!(
        e1["rule"]["id"], e2["rule"]["id"],
        "each duplicate gets its own DB-minted id"
    );
    let so1 = e1["rule"]["sort_order"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("first create must carry sort_order"))?;
    let so2 = e2["rule"]["sort_order"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("second create must carry sort_order"))?;
    assert_eq!(so2, so1 + 1, "duplicates append at max(sort_order)+1");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn patch_rule_updates_enabled_flag() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let value = format!("tc-patch-{}", uuid::Uuid::new_v4());
    let create_envelope: Value = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({"kind": "xml_tag", "value": value, "action": "drop"}))
        .send()
        .await?
        .json()
        .await?;
    let id = create_envelope["rule"]["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    let resp = client
        .patch(format!("{}/api/filter/rules/{id}", proxy.inspect_url))
        .json(&json!({"enabled": false}))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let envelope: Value = resp.json().await?;
    let updated = &envelope["rule"];
    assert_eq!(updated["enabled"], false);
    assert_eq!(updated["id"], id);
    assert_eq!(envelope["warnings"], Value::Array(vec![]));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn patch_unknown_id_distinguishes_400_from_404() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let resp = client
        .patch(format!(
            "{}/api/filter/rules/{}",
            proxy.inspect_url,
            uuid::Uuid::new_v4()
        ))
        .json(&json!({"enabled": false}))
        .send()
        .await?;
    assert_eq!(resp.status(), 400, "non-bigint id must 400");
    let body: Value = resp.json().await?;
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("valid integer"),
        "400 body must name the integer-id contract: {body:?}"
    );

    let resp = client
        .patch(format!("{}/api/filter/rules/999999999", proxy.inspect_url))
        .json(&json!({"enabled": false}))
        .send()
        .await?;
    assert_eq!(resp.status(), 404, "absent bigint id must 404");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn delete_rule_204_removes_rule() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let value = format!("tc-del-{}", uuid::Uuid::new_v4());
    let create_envelope: Value = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({"kind": "xml_tag", "value": value, "action": "drop"}))
        .send()
        .await?
        .json()
        .await?;
    let id = create_envelope["rule"]["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    let del = client
        .delete(format!("{}/api/filter/rules/{id}", proxy.inspect_url))
        .send()
        .await?;
    assert_eq!(del.status(), 204);

    let list: Vec<Value> = client
        .get(format!("{}/api/filter/rules", proxy.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    assert!(
        !list.iter().any(|r| r["id"] == Value::String(id.clone())),
        "deleted rule must be gone from the list, not kept around as a disabled edit: {list:?}"
    );

    let second_del = client
        .delete(format!("{}/api/filter/rules/{id}", proxy.inspect_url))
        .send()
        .await?;
    assert_eq!(
        second_del.status(),
        404,
        "a second delete of an already-deleted id must 404, not silently succeed again"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn delete_unknown_id_distinguishes_400_from_404() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let resp = client
        .delete(format!(
            "{}/api/filter/rules/{}",
            proxy.inspect_url,
            uuid::Uuid::new_v4()
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 400, "non-bigint id must 400");

    let resp = client
        .delete(format!("{}/api/filter/rules/999999999", proxy.inspect_url))
        .send()
        .await?;
    assert_eq!(resp.status(), 404, "absent bigint id must 404");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn create_rule_with_title_round_trips_through_get() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let value = format!("tc-title-{}", uuid::Uuid::new_v4());
    let resp = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({
            "kind": "xml_tag",
            "value": value,
            "action": "drop",
            "title": "Strip debug tags",
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 201);
    let envelope: Value = resp.json().await?;
    assert_eq!(envelope["rule"]["title"], "Strip debug tags");

    let list: Vec<Value> = client
        .get(format!("{}/api/filter/rules", proxy.inspect_url))
        .send()
        .await?
        .json()
        .await?;
    let id = envelope["rule"]["id"].clone();
    let listed = list
        .iter()
        .find(|r| r["id"] == id)
        .ok_or_else(|| anyhow::anyhow!("created rule must appear in the list"))?;
    assert_eq!(listed["title"], "Strip debug tags");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn create_rule_without_title_returns_null() -> anyhow::Result<()> {
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let value = format!("tc-notitle-{}", uuid::Uuid::new_v4());
    let resp = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({"kind": "xml_tag", "value": value, "action": "drop"}))
        .send()
        .await?;
    assert_eq!(resp.status(), 201);
    let envelope: Value = resp.json().await?;
    assert_eq!(
        envelope["rule"]["title"],
        Value::Null,
        "an omitted title must serialize as null, not be absent from the DTO"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial(filter_rules_db)]
async fn patch_rule_updates_title_then_clears_it_with_empty_string_and_null() -> anyhow::Result<()>
{
    let Some(proxy) = start_proxy_with_config(b"persist_enabled = true\n").await? else {
        return Ok(());
    };
    let client = reqwest::Client::new();

    let value = format!("tc-title-patch-{}", uuid::Uuid::new_v4());
    let create_envelope: Value = client
        .post(format!("{}/api/filter/rules", proxy.inspect_url))
        .json(&json!({"kind": "xml_tag", "value": value, "action": "drop"}))
        .send()
        .await?
        .json()
        .await?;
    let id = create_envelope["rule"]["id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert_eq!(create_envelope["rule"]["title"], Value::Null);

    let resp = client
        .patch(format!("{}/api/filter/rules/{id}", proxy.inspect_url))
        .json(&json!({"title": "Renamed rule"}))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let updated: Value = resp.json().await?;
    assert_eq!(updated["rule"]["title"], "Renamed rule");

    let resp = client
        .patch(format!("{}/api/filter/rules/{id}", proxy.inspect_url))
        .json(&json!({"title": ""}))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let cleared: Value = resp.json().await?;
    assert_eq!(
        cleared["rule"]["title"],
        Value::Null,
        "an empty-string title must be stored (and returned) as NULL, not as \"\""
    );

    let resp = client
        .patch(format!("{}/api/filter/rules/{id}", proxy.inspect_url))
        .json(&json!({"title": "Set again"}))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let reset: Value = resp.json().await?;
    assert_eq!(reset["rule"]["title"], "Set again");

    let resp = client
        .patch(format!("{}/api/filter/rules/{id}", proxy.inspect_url))
        .json(&json!({"title": null}))
        .send()
        .await?;
    assert_eq!(resp.status(), 200);
    let cleared_via_null: Value = resp.json().await?;
    assert_eq!(
        cleared_via_null["rule"]["title"],
        Value::Null,
        "an explicit JSON null must also clear the title"
    );
    Ok(())
}
