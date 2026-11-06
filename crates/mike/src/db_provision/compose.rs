use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::DbError;
use super::runner::{Call, Runner, run_checked, run_loose};

pub(crate) const CONTAINER: &str = "mike-db";
pub(crate) const VOLUME: &str = "mike_pg_data";
pub(crate) const DEFAULT_PORT: u16 = 6666;
pub(crate) const COMPOSE_FILE_NAME: &str = "docker-compose.yml";
const TEMPLATE: &str = include_str!("docker-compose.yml");
const UNUSED_PASSWORD: &str = "unused";
const LOG_LINES: &str = "20";

pub(crate) fn write_template(dir: &Path) -> Result<PathBuf, DbError> {
    let write_error = |source: io::Error| DbError::Write {
        path: dir.to_path_buf(),
        source,
    };
    std::fs::create_dir_all(dir).map_err(write_error)?;
    let file = dir.join(COMPOSE_FILE_NAME);
    crate::owner_only_file::write(&file, TEMPLATE).map_err(|source| DbError::Write {
        path: file.clone(),
        source,
    })?;
    crate::owner_only_file::write(
        &dir.join("Dockerfile.pg19"),
        include_str!("../../resources/postgres19.Dockerfile"),
    )
    .map_err(|source| DbError::Write {
        path: dir.join("Dockerfile.pg19"),
        source,
    })?;
    Ok(file)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Daemon {
    Missing,
    Down(String),
    Ready,
}

pub(crate) async fn daemon<R: Runner>(runner: &R, wait: Duration) -> Daemon {
    let call = Call::new("docker", &["info"]);
    match tokio::time::timeout(wait, runner.run(&call)).await {
        Err(_) => Daemon::Down(format!(
            "`docker info` gave no answer within {}s",
            wait.as_secs()
        )),
        Ok(Err(e)) if e.kind() == io::ErrorKind::NotFound => Daemon::Missing,
        Ok(Err(e)) => Daemon::Down(e.to_string()),
        Ok(Ok(out)) if out.success => Daemon::Ready,
        Ok(Ok(out)) => Daemon::Down(first_line(&out.detail())),
    }
}

fn first_line(text: &str) -> String {
    text.lines()
        .next()
        .unwrap_or("no details")
        .trim()
        .to_string()
}

pub(crate) async fn require_daemon<R: Runner>(runner: &R, wait: Duration) -> Result<(), DbError> {
    match daemon(runner, wait).await {
        Daemon::Ready => Ok(()),
        Daemon::Missing => Err(DbError::DockerMissing),
        Daemon::Down(detail) => Err(DbError::DockerNotRunning { detail }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ContainerState {
    Absent,
    Present {
        status: String,
        health: Option<String>,
    },
}

impl ContainerState {
    pub(crate) fn is_running(&self) -> bool {
        matches!(self, Self::Present { status, .. } if status == "running")
    }

    pub(crate) fn is_healthy(&self) -> bool {
        matches!(
            self,
            Self::Present { status, health: Some(health) }
                if status == "running" && health == "healthy"
        )
    }

    pub(crate) fn is_gone(&self) -> bool {
        matches!(self, Self::Absent)
            || matches!(self, Self::Present { status, .. } if status == "exited" || status == "dead")
    }

    pub(crate) fn describe(&self) -> String {
        match self {
            Self::Absent => "no container".to_string(),
            Self::Present {
                status,
                health: Some(health),
            } => format!("{status}, {health}"),
            Self::Present { status, .. } => status.clone(),
        }
    }

    fn parse(output: &str) -> Self {
        let (status, health) = output.trim().split_once('|').unwrap_or((output.trim(), ""));
        Self::Present {
            status: status.to_string(),
            health: Some(health.to_string()).filter(|h| !h.is_empty()),
        }
    }
}

pub(crate) async fn container_state<R: Runner>(runner: &R) -> Result<ContainerState, DbError> {
    let call = Call::new(
        "docker",
        &[
            "container",
            "inspect",
            "--format",
            "{{.State.Status}}|{{if .State.Health}}{{.State.Health.Status}}{{end}}",
            CONTAINER,
        ],
    );
    let out = run_loose(runner, &call).await?;
    if out.success {
        return Ok(ContainerState::parse(&out.stdout));
    }
    if out.detail().to_ascii_lowercase().contains("no such") {
        return Ok(ContainerState::Absent);
    }
    Err(DbError::CommandFailed {
        command: call.shown(),
        detail: out.detail(),
    })
}

pub(crate) async fn volume_exists<R: Runner>(runner: &R) -> Result<bool, DbError> {
    let call = Call::new("docker", &["volume", "inspect", VOLUME]);
    Ok(run_loose(runner, &call).await?.success)
}

pub(crate) async fn recent_logs<R: Runner>(runner: &R) -> String {
    let call = Call::new("docker", &["logs", "--tail", LOG_LINES, CONTAINER]);
    match run_loose(runner, &call).await {
        Ok(out) => format!("{}\n{}", out.stdout.trim(), out.stderr.trim())
            .trim()
            .to_string(),
        Err(e) => format!("(the container log could not be read: {e})"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flavor {
    Plugin,
    Standalone,
}

pub(crate) struct Compose<'a, R> {
    runner: &'a R,
    file: &'a Path,
    flavor: Flavor,
}

impl<'a, R: Runner> Compose<'a, R> {
    pub(crate) async fn open(runner: &'a R, file: &'a Path) -> Result<Self, DbError> {
        let plugin = Call::new("docker", &["compose", "version"]);
        if run_loose(runner, &plugin).await?.success {
            return Ok(Self {
                runner,
                file,
                flavor: Flavor::Plugin,
            });
        }
        let standalone = Call::new("docker-compose", &["version"]);
        let works = matches!(runner.run(&standalone).await, Ok(out) if out.success);
        if works {
            return Ok(Self {
                runner,
                file,
                flavor: Flavor::Standalone,
            });
        }
        Err(DbError::ComposeMissing)
    }

    fn call(&self, verb: &[&str]) -> Call {
        let file = self.file.to_string_lossy();
        let (program, lead): (&str, &[&str]) = match self.flavor {
            Flavor::Plugin => ("docker", &["compose"]),
            Flavor::Standalone => ("docker-compose", &[]),
        };
        let mut args: Vec<&str> = lead.to_vec();
        args.extend(["-f", file.as_ref()]);
        args.extend(verb);
        Call::new(program, &args)
    }

    pub(crate) async fn up(&self, port: u16, password: &str) -> Result<(), DbError> {
        let call = self
            .call(&["up", "-d", "--build"])
            .with_env("MIKE_DB_PASSWORD", password)
            .with_env("MIKE_DB_PORT", &port.to_string());
        run_checked(self.runner, &call).await.map(|_| ())
    }

    pub(crate) async fn down(&self) -> Result<(), DbError> {
        let call = self
            .call(&["down"])
            .with_env("MIKE_DB_PASSWORD", UNUSED_PASSWORD);
        run_checked(self.runner, &call).await.map(|_| ())
    }
}
