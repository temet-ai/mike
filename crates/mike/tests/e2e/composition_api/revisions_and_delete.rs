use crate::db_helpers;
use crate::upstream;

use serde_json::{Value, json};

use super::support::{
    create_via_api, find_upstream_body_for_session, prompt_sha256_for, send_canonical_request,
};

#[tokio::test]
async fn revisions_grow_on_each_patch() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();

    let prompt_sha256 = prompt_sha256_for("revisions-block2");
    let envelope = create_via_api(
        &client,
        &inspect_url,
        &prompt_sha256,
        "revisions-v1",
        "v1-text",
    )
    .await?;
    let composition_id = envelope["composition"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("composition.id absent: {envelope}"))?
        .to_string();

    let r2 = client
        .patch(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id}"
        ))
        .json(&json!({"label": "revisions-v2"}))
        .send()
        .await?;
    assert_eq!(r2.status(), 200);

    let r3 = client
        .patch(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id}"
        ))
        .json(&json!({"mode": "template"}))
        .send()
        .await?;
    assert_eq!(r3.status(), 200);

    let revs: Vec<Value> = client
        .get(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id}/revisions"
        ))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(revs.len(), 3, "expected 3 revisions, got {revs:?}");

    let mut versions: Vec<i64> = Vec::with_capacity(revs.len());
    for (i, rev) in revs.iter().enumerate() {
        let v = rev["version"]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("revision {i} missing version field: {rev}"))?;
        versions.push(v);
    }
    assert_eq!(versions, vec![3, 2, 1], "revisions must be version-DESC");

    assert_eq!(revs[2]["label"].as_str(), Some("revisions-v1"));
    assert_eq!(revs[2]["mode"].as_str(), Some("observe"));
    assert_eq!(revs[1]["label"].as_str(), Some("revisions-v2"));
    assert_eq!(revs[1]["mode"].as_str(), Some("observe"));
    assert_eq!(revs[0]["label"].as_str(), Some("revisions-v2"));
    assert_eq!(
        revs[0]["mode"].as_str(),
        Some("template"),
        "v3 must reflect the mode flip"
    );

    for rev in &revs {
        assert_eq!(
            rev["composition_id"].as_str(),
            Some(composition_id.as_str())
        );
    }

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn revisions_endpoint_returns_404_for_unknown_id() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, shutdown, pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();
    let unknown = uuid::Uuid::new_v4().hyphenated().to_string();
    let resp = client
        .get(format!(
            "{inspect_url}/api/prompts/compositions/{unknown}/revisions"
        ))
        .send()
        .await?;
    assert_eq!(resp.status(), 404);
    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn delete_referenced_returns_409_with_count() -> anyhow::Result<()> {
    let upstream = upstream::canned_json_upstream("msg_compose_pr2").await;
    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;
    let (proxy_url, inspect_url, shutdown) =
        db_helpers::start_proxy_with_db_at_composed(&upstream.uri(), &test_db.database_url).await?;
    let client = reqwest::Client::new();

    let block2_original = "delete-409-block2";
    let prompt_sha256 = prompt_sha256_for(block2_original);
    let resp = client
        .post(format!("{inspect_url}/api/prompts/compositions"))
        .json(&json!({
            "prompt_sha256": prompt_sha256,
            "label": "delete-409",
            "mode": "template",
            "blocks": [
                {"text": "billing-header"},
                {"text": "identity preamble"},
                {"text": "REWRITTEN-409-PATH"},
            ]
        }))
        .send()
        .await?;
    assert_eq!(resp.status(), 201);
    let envelope: Value = resp.json().await?;
    let composition_id = envelope["composition"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("composition.id absent: {envelope}"))?
        .to_string();

    let session_id = "deadbeef-0001-0001-0001-000000000001";
    let req = send_canonical_request(&client, &proxy_url, session_id, block2_original).await?;
    assert_eq!(req.status(), 200);

    let upstream_body = find_upstream_body_for_session(&upstream, session_id).await?;
    assert_eq!(
        upstream_body["system"][2]["text"].as_str(),
        Some("REWRITTEN-409-PATH"),
        "composition must have templated the request — FK row should now point at it"
    );

    let del = client
        .delete(format!(
            "{inspect_url}/api/prompts/compositions/{composition_id}"
        ))
        .send()
        .await?;
    assert_eq!(
        del.status(),
        409,
        "DELETE on a referenced composition must surface as 409 Conflict, not 500"
    );
    let del_body: Value = del.json().await?;
    let err = del_body["error"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("error string missing: {del_body}"))?;
    assert!(
        err.contains("1 request"),
        "409 body must include the reference count; got {err}"
    );

    let still_there: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM overrides.overrides WHERE id = $1")
            .bind(composition_id.parse::<i64>()?)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        still_there, 1,
        "409 on DELETE must NOT remove the row — caller still sees it via GET"
    );

    shutdown.cancel();
    pool.close().await;
    test_db.cleanup().await?;
    Ok(())
}
