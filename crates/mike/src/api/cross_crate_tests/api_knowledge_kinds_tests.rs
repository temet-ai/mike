use reqwest::StatusCode;
use serde_json::{Value, json};

use crate::api::rerank_search::harness::Harness;
use crate::store::persistence::test_support::seed_project;

async fn seed_widgets(harness: &Harness) -> anyhow::Result<()> {
    let pool = harness.handle.pool();
    let project = seed_project(pool, "/work/widgets").await?;
    sqlx::query(
        "CREATE TABLE public.kind_widgets (
             id bigint PRIMARY KEY, project_id bigint NOT NULL, title text NOT NULL, note text)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO public.kind_widgets (id, project_id, title, note)
         VALUES (1, $1, 'Gear', 'turns the clock'), (2, $1, 'Spring', 'stores the energy')",
    )
    .bind(project)
    .execute(pool)
    .await?;
    Ok(())
}

fn widgets_definition() -> Value {
    json!({
        "kind": "widgets",
        "label": "Widget",
        "description": "Parts of the clock",
        "source_table": "public.kind_widgets",
        "key_column": "id",
        "title_column": "title",
        "body_columns": ["note"],
        "project_column": "project_id",
    })
}

async fn kind_row(harness: &Harness, kind: &str) -> anyhow::Result<Option<Value>> {
    let (status, answer) = harness.get("/api/knowledge/kinds").await?;
    anyhow::ensure!(status.is_success(), "{answer}");
    Ok(answer["kinds"]
        .as_array()
        .and_then(|kinds| kinds.iter().find(|row| row["kind"] == kind).cloned()))
}

async fn patch(harness: &Harness, kind: &str, body: Value) -> anyhow::Result<(StatusCode, Value)> {
    let response = harness
        .client
        .patch(format!("{}/api/knowledge/kinds/{kind}", harness.base))
        .json(&body)
        .send()
        .await?;
    Ok((response.status(), response.json().await?))
}

async fn delete(harness: &Harness, kind: &str) -> anyhow::Result<(StatusCode, Option<Value>)> {
    let response = harness
        .client
        .delete(format!("{}/api/knowledge/kinds/{kind}", harness.base))
        .send()
        .await?;
    let status = response.status();
    Ok((status, response.json().await.ok()))
}

async fn items_of(harness: &Harness, kind: &str) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM memory.knowledge_items WHERE kind = $1")
            .bind(kind)
            .fetch_one(harness.handle.pool())
            .await?,
    )
}

#[tokio::test]
async fn kinds_define_over_a_table_lists_the_kind_with_its_items_counted() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    seed_widgets(&harness).await?;

    let (status, answer) = harness
        .post("/api/knowledge/kinds", widgets_definition())
        .await?;

    assert_eq!(status, StatusCode::CREATED, "{answer}");
    assert_eq!(answer["kind"], "widgets");
    let row = kind_row(&harness, "widgets")
        .await?
        .ok_or_else(|| anyhow::anyhow!("widgets is not listed"))?;
    assert_eq!(row["label"], "Widget");
    assert_eq!(row["built_in"], false);
    assert_eq!(row["enabled"], true);
    assert_eq!(row["generated_view"], true);
    assert_eq!(row["stats"]["items"], 2);
    assert_eq!(row["definition"]["source_table"], "public.kind_widgets");
    assert_eq!(row["definition"]["body_columns"], json!(["note"]));
    assert_eq!(row["watches"][0]["table"], "public.kind_widgets");
    assert!(row["watches"][0]["watch_columns"].is_array());
    harness.cleanup().await
}

#[tokio::test]
async fn kinds_list_gives_built_in_kinds_a_null_definition() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;

    let (status, answer) = harness.get("/api/knowledge/kinds").await?;

    assert!(status.is_success(), "{answer}");
    let built_in: Vec<&Value> = answer["kinds"]
        .as_array()
        .map(|kinds| kinds.iter().filter(|row| row["built_in"] == true).collect())
        .unwrap_or_default();
    assert!(!built_in.is_empty());
    assert!(built_in.iter().all(|row| row["definition"].is_null()));
    harness.cleanup().await
}

#[tokio::test]
async fn kinds_tables_lists_the_table_with_its_column_types() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    seed_widgets(&harness).await?;

    let (status, answer) = harness.get("/api/knowledge/kinds/tables").await?;

    assert!(status.is_success(), "{answer}");
    let table = answer["tables"]
        .as_array()
        .and_then(|tables| tables.iter().find(|t| t["name"] == "public.kind_widgets"))
        .ok_or_else(|| anyhow::anyhow!("public.kind_widgets is not listed: {answer}"))?;
    assert!(
        table["columns"]
            .as_array()
            .is_some_and(|columns| columns.contains(&json!({"name": "id", "type": "bigint"})))
    );
    harness.cleanup().await
}

#[tokio::test]
async fn kinds_define_with_a_missing_column_is_a_400_naming_the_field() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    seed_widgets(&harness).await?;
    let mut definition = widgets_definition();
    definition["body_columns"] = json!(["note", "no_such_column"]);

    let (status, answer) = harness.post("/api/knowledge/kinds", definition).await?;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    assert_eq!(answer["field"], "body_columns");
    assert!(
        answer["error"]
            .as_str()
            .is_some_and(|error| error.contains("no_such_column"))
    );
    assert!(answer["watch_index"].is_null());
    assert!(kind_row(&harness, "widgets").await?.is_none());
    harness.cleanup().await
}

#[tokio::test]
async fn kinds_preview_shows_one_item_and_defines_nothing() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    seed_widgets(&harness).await?;

    let (status, answer) = harness
        .post("/api/knowledge/kinds/preview", widgets_definition())
        .await?;

    assert_eq!(status, StatusCode::OK, "{answer}");
    assert!(
        answer["preview"]["reference"]
            .as_str()
            .is_some_and(|reference| reference.starts_with("widgets:"))
    );
    assert!(answer["preview"]["body"].is_string());
    assert!(kind_row(&harness, "widgets").await?.is_none());
    harness.cleanup().await
}

#[tokio::test]
async fn kinds_patch_switches_a_kind_off_and_on_and_returns_its_row() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    seed_widgets(&harness).await?;
    harness
        .post("/api/knowledge/kinds", widgets_definition())
        .await?;

    let (off_status, off) = patch(&harness, "widgets", json!({"enabled": false})).await?;
    let (on_status, on) = patch(
        &harness,
        "widgets",
        json!({"enabled": true, "settle_secs": 30}),
    )
    .await?;

    assert_eq!(off_status, StatusCode::OK, "{off}");
    assert_eq!(off["kind"], "widgets");
    assert_eq!(off["enabled"], false);
    assert_eq!(on_status, StatusCode::OK, "{on}");
    assert_eq!(on["enabled"], true);
    assert_eq!(on["settle_secs"], 30);
    assert_eq!(on["stats"]["items"], 2);
    harness.cleanup().await
}

#[tokio::test]
async fn kinds_patch_an_unknown_kind_is_a_400_naming_the_kind() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;

    let (status, answer) = patch(&harness, "no_such_kind", json!({"enabled": false})).await?;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    assert_eq!(answer["field"], "kind");
    harness.cleanup().await
}

#[tokio::test]
async fn kinds_delete_a_built_in_kind_is_a_400_and_keeps_it() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    let (_, listed) = harness.get("/api/knowledge/kinds").await?;
    let built_in = listed["kinds"]
        .as_array()
        .and_then(|kinds| kinds.iter().find(|row| row["built_in"] == true))
        .and_then(|row| row["kind"].as_str())
        .ok_or_else(|| anyhow::anyhow!("no built-in kind is listed: {listed}"))?
        .to_owned();

    let (status, answer) = delete(&harness, &built_in).await?;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer:?}");
    assert!(kind_row(&harness, &built_in).await?.is_some());
    harness.cleanup().await
}

#[tokio::test]
async fn kinds_delete_a_custom_kind_removes_it_and_its_items() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    seed_widgets(&harness).await?;
    harness
        .post("/api/knowledge/kinds", widgets_definition())
        .await?;
    assert_eq!(items_of(&harness, "widgets").await?, 2);

    let (status, _) = delete(&harness, "widgets").await?;

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(kind_row(&harness, "widgets").await?.is_none());
    assert_eq!(items_of(&harness, "widgets").await?, 0);
    harness.cleanup().await
}
