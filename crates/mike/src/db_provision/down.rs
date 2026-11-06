use super::DbError;
use super::compose::{COMPOSE_FILE_NAME, Compose, VOLUME, require_daemon};
use super::runner::{Call, Runner, run_checked};
use super::up::{Paths, Timing};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DownOutcome {
    NothingToStop,
    Stopped,
}

pub(crate) async fn down<R: Runner>(
    runner: &R,
    paths: &Paths,
    timing: Timing,
) -> Result<DownOutcome, DbError> {
    let file = paths.compose_dir().join(COMPOSE_FILE_NAME);
    if !file.is_file() {
        return Ok(DownOutcome::NothingToStop);
    }
    require_daemon(runner, timing.daemon_wait).await?;
    Compose::open(runner, &file).await?.down().await?;
    Ok(DownOutcome::Stopped)
}

pub(crate) async fn remove_volume<R: Runner>(runner: &R) -> Result<(), DbError> {
    run_checked(runner, &Call::new("docker", &["volume", "rm", VOLUME]))
        .await
        .map(|_| ())
}
