use super::{Daemon, Docker, home, quick};
use crate::db_provision::DbError;
use crate::db_provision::compose::write_template;
use crate::db_provision::credentials::save_database_url;
use crate::db_provision::down::{DownOutcome, down};
use crate::db_provision::status::{gather, render};

#[tokio::test]
async fn down_without_a_compose_file_has_nothing_to_stop_and_runs_no_command() -> anyhow::Result<()>
{
    let home = home()?;
    let docker = Docker::new(&["absent"]);
    let outcome = down(&docker, &home.paths, quick()).await?;
    assert_eq!(outcome, DownOutcome::NothingToStop);
    assert!(docker.calls().is_empty());
    Ok(())
}

#[tokio::test]
async fn down_stops_the_container_and_never_asks_for_the_volume_to_go() -> anyhow::Result<()> {
    let home = home()?;
    write_template(&home.paths.compose_dir())?;
    let docker = Docker::new(&["running|healthy"]);
    let outcome = down(&docker, &home.paths, quick()).await?;
    assert_eq!(outcome, DownOutcome::Stopped);
    let stop = docker
        .call_with("down")
        .ok_or_else(|| anyhow::anyhow!("no compose down call in {:?}", docker.shown()))?;
    assert!(
        stop.args.iter().all(|a| a != "-v" && a != "--volumes"),
        "{:?}",
        stop.args
    );
    Ok(())
}

#[tokio::test]
async fn down_with_the_daemon_stopped_reports_it() -> anyhow::Result<()> {
    let home = home()?;
    write_template(&home.paths.compose_dir())?;
    let mut docker = Docker::new(&["absent"]);
    docker.daemon = Daemon::Down;
    let result = down(&docker, &home.paths, quick()).await;
    assert!(
        matches!(result, Err(DbError::DockerNotRunning { .. })),
        "{result:?}"
    );
    Ok(())
}

#[tokio::test]
async fn status_describes_a_running_database_without_showing_its_password() -> anyhow::Result<()> {
    let home = home()?;
    save_database_url(
        &home.paths.config_file,
        "postgres://mike:topsecret@127.0.0.1:6666/mike",
    )?;
    write_template(&home.paths.compose_dir())?;
    let docker = Docker::new(&["running|healthy"]);
    let status = gather(&docker, &home.paths, quick()).await?;
    let text = render(&status);
    assert!(
        text.contains("container:       mike-db: running, healthy"),
        "{text}"
    );
    assert!(text.contains("mike_pg_data: present"), "{text}");
    assert!(
        text.contains("postgres://mike:***@127.0.0.1:6666/mike"),
        "{text}"
    );
    assert!(!text.contains("topsecret"), "{text}");
    assert!(!text.contains("does not point"), "{text}");
    Ok(())
}

#[tokio::test]
async fn status_with_docker_missing_and_a_foreign_url_says_both() -> anyhow::Result<()> {
    let home = home()?;
    save_database_url(
        &home.paths.config_file,
        "postgres://ada:pw@db.example:5432/x",
    )?;
    let mut docker = Docker::new(&["absent"]);
    docker.daemon = Daemon::Missing;
    let text = render(&gather(&docker, &home.paths, quick()).await?);
    assert!(text.contains("docker:          not installed"), "{text}");
    assert!(
        text.contains("does not point at the database `mike db up` runs"),
        "{text}"
    );
    assert!(!text.contains("container:"), "{text}");
    Ok(())
}
