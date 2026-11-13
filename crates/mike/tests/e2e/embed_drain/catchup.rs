use super::*;

#[tokio::test]
async fn idle_catchup_drains_multiple_batches_and_active_catchup_limits_work() -> anyhow::Result<()>
{
    let upstream = MockServer::start().await;
    let Some(proxy) = db_helpers::start_proxy_with_db(&upstream.uri()).await? else {
        return Ok(());
    };
    let (_, _, initial_shutdown, pool, test_db) = proxy.into_parts();
    initial_shutdown.cancel();
    let embedding = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(echo_count_responder())
        .mount(&embedding)
        .await;
    let session = "12345678-d003-4d03-8d03-d03d03d03d03";
    let project = bootstrap_project_session(&pool, "test-embed-catchup", session).await?;
    let old = chrono::Utc::now() - chrono::Duration::minutes(10);
    for index in 0..40 {
        insert_closed_turn_at(
            &pool,
            session,
            project,
            &format!("Explain binary search case {index}"),
            "Binary search halves the sorted search space on each step.",
            old,
        )
        .await?;
    }
    let (_, inspect, shutdown, state) =
        start_proxy_with_state(&upstream.uri(), &test_db.database_url).await?;
    patch_embedding_setting(&inspect, "embedding.enabled", serde_json::json!(true)).await?;
    patch_embedding_setting(
        &inspect,
        "embedding.url",
        serde_json::json!(embedding.uri()),
    )
    .await?;
    assert_eq!(
        mike::embed_worker::run_catchup_once(&state.knowledge_state()).await,
        40
    );
    for index in 0..20 {
        insert_closed_turn(
            &pool,
            session,
            project,
            &format!("Explain active binary search case {index}"),
            "The algorithm searches a sorted collection in logarithmic time.",
        )
        .await?;
    }
    assert_eq!(
        mike::embed_worker::run_catchup_once(&state.knowledge_state()).await,
        16
    );
    assert_eq!(
        mike::embed_worker::run_catchup_once(&state.knowledge_state()).await,
        4
    );
    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}
