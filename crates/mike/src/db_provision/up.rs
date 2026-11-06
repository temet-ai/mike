use std::path::PathBuf;
use std::time::Duration;

use super::DbError;
use super::compose::{
    Compose, ContainerState, DEFAULT_PORT, container_state, recent_logs, require_daemon,
    write_template,
};
use super::credentials::{
    Managed, generate_password, managed_from_url, save_database_url, stored_database_url, url_for,
};
use super::runner::Runner;
use super::server::{Server, ServerError};
use crate::config::load::redact_database_url;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Timing {
    pub(crate) daemon_wait: Duration,
    pub(crate) health_polls: u32,
    pub(crate) poll: Duration,
}

impl Timing {
    pub(crate) const REAL: Timing = Timing {
        daemon_wait: Duration::from_secs(20),
        health_polls: 60,
        poll: Duration::from_secs(2),
    };

    pub(crate) fn health_wait_secs(&self) -> u64 {
        (self.poll * self.health_polls).as_secs()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Paths {
    pub(crate) config_file: PathBuf,
    pub(crate) state_dir: PathBuf,
}

impl Paths {
    pub(crate) fn from_environment() -> Result<Self, DbError> {
        Ok(Self {
            config_file: crate::user_config::resolved_config_file()?,
            state_dir: crate::user_config::state_dir()?,
        })
    }

    pub(crate) fn compose_dir(&self) -> PathBuf {
        self.state_dir.join("db")
    }
}

pub(crate) struct Context<'a, R, S> {
    pub(crate) runner: &'a R,
    pub(crate) server: &'a S,
    pub(crate) paths: &'a Paths,
    pub(crate) timing: Timing,
    pub(crate) port_free: fn(u16) -> bool,
    pub(crate) progress: fn(&str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpOutcome {
    pub(crate) url: String,
    pub(crate) port: u16,
    pub(crate) already_running: bool,
    pub(crate) replaced_url: Option<String>,
    pub(crate) migrations_applied: usize,
}

pub(crate) fn port_is_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

pub(crate) async fn up<R: Runner, S: Server>(
    ctx: &Context<'_, R, S>,
    requested_port: Option<u16>,
) -> Result<UpOutcome, DbError> {
    let file = write_template(&ctx.paths.compose_dir())?;
    require_daemon(ctx.runner, ctx.timing.daemon_wait).await?;
    let compose = Compose::open(ctx.runner, &file).await?;
    let stored = stored_database_url(&ctx.paths.config_file)?;
    let managed = stored.as_deref().and_then(managed_from_url);
    let port = requested_port
        .or(managed.as_ref().map(|m| m.port))
        .unwrap_or(DEFAULT_PORT);
    let already_running = already_on_port(ctx, managed.as_ref(), port).await?;
    if !already_running && !(ctx.port_free)(port) {
        return Err(DbError::PortInUse { port });
    }
    let password = managed
        .as_ref()
        .map_or_else(generate_password, |m| m.password.clone());
    (ctx.progress)(
        "starting PostgreSQL 19 with pgvector (downloading the image first can take a few minutes)",
    );
    compose.up(port, &password).await?;
    wait_healthy(ctx).await?;
    (ctx.progress)("preparing the schema with mike's own migrations");
    let url = url_for(&password, port);
    let migrations_applied = prepare_schema(ctx, &url).await?;
    save_database_url(&ctx.paths.config_file, &url)?;
    Ok(UpOutcome {
        url,
        port,
        already_running,
        replaced_url: stored
            .filter(|_| managed.is_none())
            .map(|old| redact_database_url(&old)),
        migrations_applied,
    })
}

async fn already_on_port<R: Runner, S: Server>(
    ctx: &Context<'_, R, S>,
    managed: Option<&Managed>,
    port: u16,
) -> Result<bool, DbError> {
    let same_port = managed.is_some_and(|m| m.port == port);
    Ok(same_port && container_state(ctx.runner).await?.is_running())
}

async fn prepare_schema<R: Runner, S: Server>(
    ctx: &Context<'_, R, S>,
    url: &str,
) -> Result<usize, DbError> {
    match ctx.server.migrate(url).await {
        Ok(applied) => Ok(applied),
        Err(ServerError::Auth(_)) => Err(DbError::PasswordMismatch),
        Err(ServerError::Other(why)) => {
            save_database_url(&ctx.paths.config_file, url)?;
            Err(DbError::Migrate(why))
        }
    }
}

async fn wait_healthy<R: Runner, S: Server>(ctx: &Context<'_, R, S>) -> Result<(), DbError> {
    let mut last = ContainerState::Absent;
    for _ in 0..ctx.timing.health_polls {
        last = container_state(ctx.runner).await?;
        if last.is_healthy() {
            return Ok(());
        }
        if last.is_gone() {
            break;
        }
        tokio::time::sleep(ctx.timing.poll).await;
    }
    Err(DbError::NotHealthy {
        waited_secs: ctx.timing.health_wait_secs(),
        state: last.describe(),
        logs: recent_logs(ctx.runner).await,
    })
}
