use mike::app::{inspect_router_on, proxy_router};
use mike::config::{AppState, ProxyConfig};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

pub(crate) use crate::test_db::{TestDb, TestDbHandle};

#[allow(dead_code)]
pub struct ProxyWithDb {
    pub proxy_url: String,
    pub inspect_url: String,
    pub shutdown: CancellationToken,
    pub pool: sqlx::PgPool,
    pub test_db: TestDbHandle,
}

impl ProxyWithDb {
    #[allow(dead_code)]
    pub fn into_parts(
        self,
    ) -> (
        String,
        String,
        CancellationToken,
        sqlx::PgPool,
        TestDbHandle,
    ) {
        (
            self.proxy_url,
            self.inspect_url,
            self.shutdown,
            self.pool,
            self.test_db,
        )
    }
}

#[allow(dead_code)]
pub async fn start_proxy_with_db(upstream_url: &str) -> anyhow::Result<Option<ProxyWithDb>> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    finish_proxy_with_db(upstream_url, test_db).await.map(Some)
}

#[allow(dead_code)]
pub async fn start_proxy_with_shared_db(upstream_url: &str) -> anyhow::Result<Option<ProxyWithDb>> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let test_db = test_db_factory.acquire_shared().await?;
    finish_proxy_with_db(upstream_url, test_db).await.map(Some)
}

async fn finish_proxy_with_db(
    upstream_url: &str,
    test_db: TestDbHandle,
) -> anyhow::Result<ProxyWithDb> {
    let (proxy_url, inspect_url, shutdown) =
        start_proxy_with_db_at(upstream_url, &test_db.database_url).await?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    Ok(ProxyWithDb {
        proxy_url,
        inspect_url,
        shutdown,
        pool,
        test_db,
    })
}

#[allow(dead_code)]
pub async fn start_proxy_with_shared_db_no_local_fallback(
    upstream_url: &str,
) -> anyhow::Result<Option<ProxyWithDb>> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let test_db = test_db_factory.acquire_shared().await?;
    let (proxy_url, inspect_url, shutdown) =
        start_proxy_with_db_at_no_local_fallback(upstream_url, &test_db.database_url).await?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    Ok(Some(ProxyWithDb {
        proxy_url,
        inspect_url,
        shutdown,
        pool,
        test_db,
    }))
}

#[allow(dead_code)]
async fn start_proxy_with_db_at_no_local_fallback(
    upstream_url: &str,
    database_url: &str,
) -> anyhow::Result<(String, String, CancellationToken)> {
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        upstream_url,
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        database_url,
        "--db-fallback-enabled",
        "0",
    ])?;
    serve_config(config).await
}

pub async fn start_proxy_with_db_at(
    upstream_url: &str,
    database_url: &str,
) -> anyhow::Result<(String, String, CancellationToken)> {
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        upstream_url,
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        database_url,
    ])?;
    serve_config(config).await
}

#[allow(dead_code)]
pub async fn start_proxy_with_db_at_composed(
    upstream_url: &str,
    database_url: &str,
) -> anyhow::Result<(String, String, CancellationToken)> {
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        upstream_url,
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        database_url,
        "--composition-enabled",
        "1",
    ])?;
    serve_config(config).await
}

async fn serve_config(config: ProxyConfig) -> anyhow::Result<(String, String, CancellationToken)> {
    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_addr = proxy_listener.local_addr()?;
    let proxy_app = proxy_router(state.clone());

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_addr = inspect_listener.local_addr()?;
    let inspect_app = inspect_router_on(state, inspect_addr);

    let proxy_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(proxy_listener, proxy_app)
            .with_graceful_shutdown(async move { proxy_shutdown.cancelled().await })
            .await
            .ok();
    });

    let inspect_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(inspect_listener, inspect_app)
            .with_graceful_shutdown(async move { inspect_shutdown.cancelled().await })
            .await
            .ok();
    });

    Ok((
        format!("http://{proxy_addr}"),
        format!("http://{inspect_addr}"),
        shutdown,
    ))
}
