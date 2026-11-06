use crate::db_helpers;

#[tokio::test]
async fn lessons_delete_invalid_uuid_returns_400() -> anyhow::Result<()> {
    let mock = wiremock::MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&mock.uri()).await? else {
        return Ok(());
    };
    let (_proxy_url, inspect_url, _shutdown, _pool, test_db) = proxy.into_parts();
    let client = reqwest::Client::new();
    let resp = client
        .delete(format!("{}/api/lessons/not-a-uuid", inspect_url))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
    test_db.cleanup().await?;
    Ok(())
}
