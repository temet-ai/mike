use super::*;

#[tokio::test]
async fn run_retries_connect_failure_until_shutdown_cancelled() -> anyhow::Result<()> {
    let (tx, _rx) = broadcast::channel(8);
    let shutdown = CancellationToken::new();
    let task_shutdown = shutdown.clone();
    let join = tokio::spawn(run(
        "not-a-valid-postgres-url".to_string(),
        tx,
        task_shutdown,
    ));

    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(
        !join.is_finished(),
        "run() must not exit on a connect failure — it should still be retrying"
    );

    shutdown.cancel();
    let join_result = tokio::time::timeout(Duration::from_secs(5), join)
        .await
        .map_err(|_| anyhow::anyhow!("run() did not exit within 5s of shutdown"))?;
    join_result??;
    Ok(())
}
