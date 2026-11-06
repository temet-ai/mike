use crate::anthropic_fixtures;
use crate::test_db;

use std::path::PathBuf;
use std::sync::Arc;

use anthropic_fixtures::MockResponse;
use mike::app::{inspect_router_on, proxy_router};
use mike::config::AppState;
use test_db::{TestDb, TestDbHandle};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, OwnedMutexGuard};

fn env_lock() -> Arc<Mutex<()>> {
    static LOCK: std::sync::OnceLock<Arc<Mutex<()>>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| Arc::new(Mutex::new(()))).clone()
}

pub(super) async fn acquire_env_lock() -> OwnedMutexGuard<()> {
    env_lock().lock_owned().await
}

pub(super) async fn lazy_mock_upstream() -> wiremock::MockServer {
    use wiremock::matchers::{method, path};
    let mock_server = wiremock::MockServer::start().await;
    wiremock::Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_lazy_test")
                        .text("Hi!")
                        .usage(10, 3)
                        .build(),
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;
    mock_server
}

pub(super) struct EnvGuard {
    key: &'static str,
    prior: Option<std::ffi::OsString>,
}

impl EnvGuard {
    pub(super) fn set(key: &'static str, value: &std::path::Path) -> Self {
        let prior = std::env::var_os(key);
        unsafe { std::env::set_var(key, value) };
        Self { key, prior }
    }

    pub(super) fn clear(key: &'static str) -> Self {
        let prior = std::env::var_os(key);
        unsafe { std::env::remove_var(key) };
        Self { key, prior }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prior {
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

pub(super) async fn start_proxy_with_config(
    fixture_bytes: &[u8],
) -> anyhow::Result<Option<StartedProxy>> {
    start_proxy_with_config_upstream(fixture_bytes, "http://127.0.0.1:0").await
}

pub(super) async fn start_proxy_with_config_upstream(
    fixture_bytes: &[u8],
    upstream_url: &str,
) -> anyhow::Result<Option<StartedProxy>> {
    let Some(test_db_factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let test_db = test_db_factory.acquire_isolated().await?;
    let tmp = tempfile::TempDir::new()?;
    let config_path = tmp.path().join("config.toml");
    std::fs::write(&config_path, fixture_bytes)?;

    let serial = acquire_env_lock().await;
    let env_guard = EnvGuard::set("MIKE_CONFIG_DIR", tmp.path());
    let home_guard = EnvGuard::clear("HOME");
    let xdg_guard = EnvGuard::clear("XDG_CONFIG_HOME");

    use clap::Parser;
    let config_arg = config_path.to_string_lossy().into_owned();
    let overrides = mike::config::CliOverrides::try_parse_from([
        "mike",
        "--config",
        config_arg.as_str(),
        "--upstream-url",
        upstream_url,
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        test_db.database_url.as_str(),
    ])?;
    let config = mike::config::load(overrides)?;
    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_addr = proxy_listener.local_addr()?;
    let proxy_app = proxy_router(state.clone());

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_addr = inspect_listener.local_addr()?;
    let inspect_app = inspect_router_on(state, inspect_listener.local_addr()?);

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

    Ok(Some(StartedProxy {
        proxy_url: format!("http://{proxy_addr}"),
        inspect_url: format!("http://{inspect_addr}"),
        config_path,
        _config_tmp: tmp,
        test_db,
        _env_guard: env_guard,
        _home_guard: home_guard,
        _xdg_guard: xdg_guard,
        shutdown,
        _serial_guard: serial,
    }))
}

pub(super) struct StartedProxy {
    pub(super) proxy_url: String,
    pub(super) inspect_url: String,
    pub(super) config_path: PathBuf,
    _config_tmp: tempfile::TempDir,
    pub(super) test_db: TestDbHandle,
    pub(super) shutdown: tokio_util::sync::CancellationToken,
    _env_guard: EnvGuard,
    _home_guard: EnvGuard,
    _xdg_guard: EnvGuard,
    _serial_guard: OwnedMutexGuard<()>,
}

impl Drop for StartedProxy {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

pub(super) mod remediation_helpers {
    use super::{
        AppState, EnvGuard, OwnedMutexGuard, PathBuf, TcpListener, TestDb, TestDbHandle,
        acquire_env_lock, inspect_router_on, proxy_router,
    };

    use wiremock::MockServer;

    pub(crate) async fn mount_ok_upstream() -> MockServer {
        crate::upstream::canned_json_upstream("msg_remediation").await
    }

    pub(crate) async fn start_proxy_with_upstream(
        fixture_bytes: &[u8],
        upstream_url: &str,
    ) -> anyhow::Result<Option<StartedProxyWithUpstream>> {
        let Some(test_db_factory) = TestDb::bootstrap().await? else {
            return Ok(None);
        };
        let test_db = test_db_factory.acquire_isolated().await?;
        let tmp = tempfile::TempDir::new()?;
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, fixture_bytes)?;

        let serial = acquire_env_lock().await;
        let env_guard = EnvGuard::set("MIKE_CONFIG_DIR", tmp.path());
        let home_guard = EnvGuard::clear("HOME");
        let xdg_guard = EnvGuard::clear("XDG_CONFIG_HOME");

        use clap::Parser;
        let config_arg = config_path.to_string_lossy().into_owned();
        let overrides = mike::config::CliOverrides::try_parse_from([
            "mike",
            "--config",
            config_arg.as_str(),
            "--upstream-url",
            upstream_url,
            "--proxy-listen-addr",
            "127.0.0.1:0",
            "--inspect-listen-addr",
            "127.0.0.1:0",
            "--persist-enabled",
            "1",
            "--database-url",
            test_db.database_url.as_str(),
        ])?;
        let config = mike::config::load(overrides)?;
        let state = AppState::new(config).await?;
        let shutdown = state.shutdown();

        let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
        let proxy_addr = proxy_listener.local_addr()?;
        let proxy_app = proxy_router(state.clone());

        let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
        let inspect_addr = inspect_listener.local_addr()?;
        let inspect_app = inspect_router_on(state, inspect_listener.local_addr()?);

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

        Ok(Some(StartedProxyWithUpstream {
            proxy_url: format!("http://{proxy_addr}"),
            inspect_url: format!("http://{inspect_addr}"),
            _config_path: config_path,
            _config_tmp: tmp,
            _test_db: test_db,
            _env_guard: env_guard,
            _home_guard: home_guard,
            _xdg_guard: xdg_guard,
            shutdown,
            _serial_guard: serial,
        }))
    }

    pub(crate) struct StartedProxyWithUpstream {
        pub(crate) proxy_url: String,
        pub(crate) inspect_url: String,
        _config_path: PathBuf,
        _config_tmp: tempfile::TempDir,
        _test_db: TestDbHandle,
        pub(crate) shutdown: tokio_util::sync::CancellationToken,
        _env_guard: EnvGuard,
        _home_guard: EnvGuard,
        _xdg_guard: EnvGuard,
        _serial_guard: OwnedMutexGuard<()>,
    }

    impl Drop for StartedProxyWithUpstream {
        fn drop(&mut self) {
            self.shutdown.cancel();
        }
    }
}
