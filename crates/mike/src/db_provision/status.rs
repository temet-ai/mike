use std::fmt::Write;
use std::path::PathBuf;

use super::DbError;
use super::compose::{
    COMPOSE_FILE_NAME, CONTAINER, ContainerState, Daemon, VOLUME, container_state, daemon,
    volume_exists,
};
use super::credentials::{managed_from_url, stored_database_url};
use super::runner::Runner;
use super::up::{Paths, Timing};
use crate::config::load::redact_database_url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Status {
    pub(crate) docker: Daemon,
    pub(crate) container: Option<ContainerState>,
    pub(crate) volume_present: Option<bool>,
    pub(crate) configured_url: Option<String>,
    pub(crate) url_is_managed_by_mike_db: bool,
    pub(crate) compose_file: PathBuf,
    pub(crate) compose_file_present: bool,
}

pub(crate) async fn gather<R: Runner>(
    runner: &R,
    paths: &Paths,
    timing: Timing,
) -> Result<Status, DbError> {
    let compose_file = paths.compose_dir().join(COMPOSE_FILE_NAME);
    let stored = stored_database_url(&paths.config_file)?;
    let docker = daemon(runner, timing.daemon_wait).await;
    let (container, volume_present) = if docker == Daemon::Ready {
        (
            Some(container_state(runner).await?),
            Some(volume_exists(runner).await?),
        )
    } else {
        (None, None)
    };
    Ok(Status {
        docker,
        container,
        volume_present,
        url_is_managed_by_mike_db: stored.as_deref().and_then(managed_from_url).is_some(),
        configured_url: stored.as_deref().map(redact_database_url),
        compose_file_present: compose_file.is_file(),
        compose_file,
    })
}

pub(crate) fn render(status: &Status) -> String {
    let mut out = String::new();
    let docker = match &status.docker {
        Daemon::Ready => "running".to_string(),
        Daemon::Missing => "not installed".to_string(),
        Daemon::Down(why) => format!("not answering ({why})"),
    };
    let _ = writeln!(out, "docker:          {docker}");
    if let Some(container) = &status.container {
        let _ = writeln!(
            out,
            "container:       {CONTAINER}: {}",
            container.describe()
        );
    }
    if let Some(present) = status.volume_present {
        let volume = if present { "present" } else { "not created" };
        let _ = writeln!(out, "data volume:     {VOLUME}: {volume}");
    }
    let url = status.configured_url.as_deref().unwrap_or("not set");
    let _ = writeln!(out, "database_url:    {url}");
    if status.configured_url.is_some() && !status.url_is_managed_by_mike_db {
        let _ = writeln!(
            out,
            "                 this URL does not point at the database `mike db up` runs"
        );
    }
    let file = status.compose_file.display();
    let written = if status.compose_file_present {
        "written"
    } else {
        "not written yet"
    };
    let _ = writeln!(out, "compose file:    {file} ({written})");
    out
}
