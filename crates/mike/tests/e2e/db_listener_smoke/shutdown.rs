use super::harness::{start_proxy_with_listener, ws_url};

use tokio_tungstenite::connect_async;

#[tokio::test]
async fn cancellation_token_shuts_listener_down_within_one_second() -> anyhow::Result<()> {
    let Some(handle) = start_proxy_with_listener().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping db_listener_smoke");
        return Ok(());
    };

    let (ws, _resp) = connect_async(ws_url(&handle.inspect_url)).await?;
    drop(ws);

    let inspect_url = handle.inspect_url.clone();
    let shutdown = handle.shutdown.clone();
    shutdown.cancel();

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
    let mut accepted_after_cancel = true;
    while tokio::time::Instant::now() < deadline {
        match connect_async(ws_url(&inspect_url)).await {
            Ok((ws, _)) => {
                drop(ws);
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            Err(_) => {
                accepted_after_cancel = false;
                break;
            }
        }
    }
    assert!(
        !accepted_after_cancel,
        "inspect bind still accepting connections 1 s after cancellation"
    );

    handle.cleanup().await?;
    Ok(())
}
