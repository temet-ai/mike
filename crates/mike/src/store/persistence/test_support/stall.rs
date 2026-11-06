use std::time::Duration;

use sqlx::PgPool;
use tokio::task::JoinHandle;

pub async fn starve_pool(pool: &PgPool, hold: Duration) -> anyhow::Result<JoinHandle<()>> {
    let mut held = Vec::new();
    for _ in 0..pool.options().get_max_connections() {
        held.push(pool.acquire().await?);
    }
    Ok(tokio::spawn(async move {
        tokio::time::sleep(hold).await;
        drop(held);
    }))
}
