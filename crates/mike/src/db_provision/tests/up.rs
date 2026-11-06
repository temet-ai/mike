use super::{ComposeCli, Daemon, Docker, FakeServer, Reply, home, lock, quick, read_config};
use crate::db_provision::DbError;
use crate::db_provision::up::{Context, UpOutcome, up};

fn free(_: u16) -> bool {
    true
}

fn taken(_: u16) -> bool {
    false
}

fn silent(_: &str) {}

fn context<'a>(
    docker: &'a Docker,
    server: &'a FakeServer,
    paths: &'a crate::db_provision::up::Paths,
    port_free: fn(u16) -> bool,
) -> Context<'a, Docker, FakeServer> {
    Context {
        runner: docker,
        server,
        paths,
        timing: quick(),
        port_free,
        progress: silent,
    }
}

fn password_of(outcome: &UpOutcome) -> String {
    outcome
        .url
        .split(':')
        .nth(2)
        .and_then(|rest| rest.split('@').next())
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn up_on_a_fresh_machine_starts_postgres_waits_and_saves_a_working_url() -> anyhow::Result<()>
{
    let home = home()?;
    let docker = Docker::new(&["running|starting", "running|healthy"]);
    let server = FakeServer::new(Reply::Applied(12));
    let outcome = up(&context(&docker, &server, &home.paths, free), None).await?;

    let password = password_of(&outcome);
    assert_eq!(password.len(), 64, "{}", outcome.url);
    assert!(password.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(
        outcome.url,
        format!("postgres://mike:{password}@127.0.0.1:6666/mike")
    );
    assert_eq!(outcome.port, 6666);
    assert_eq!(outcome.migrations_applied, 12);
    assert!(!outcome.already_running);
    assert_eq!(*lock(&server.urls), vec![outcome.url.clone()]);

    let config = read_config(&home.paths);
    assert!(config.contains(&outcome.url), "{config}");
    assert!(config.contains("persist_enabled = true"), "{config}");

    let compose_file = home.paths.compose_dir().join("docker-compose.yml");
    let compose = std::fs::read_to_string(&compose_file)?;
    assert!(
        compose.contains("image: mike/postgres-pgvector:19rc1-0.8.7"),
        "{compose}"
    );
    assert!(
        compose.contains("\"127.0.0.1:${MIKE_DB_PORT:-6666}:5432\""),
        "{compose}"
    );
    assert!(compose.contains("name: mike_pg_data"), "{compose}");
    Ok(())
}

#[tokio::test]
async fn up_gives_the_password_to_compose_through_the_environment_only() -> anyhow::Result<()> {
    let home = home()?;
    let docker = Docker::new(&["running|healthy"]);
    let server = FakeServer::new(Reply::Applied(0));
    let outcome = up(&context(&docker, &server, &home.paths, free), None).await?;
    let password = password_of(&outcome);

    let start = docker
        .call_with("up")
        .ok_or_else(|| anyhow::anyhow!("no compose up call in {:?}", docker.shown()))?;
    assert_eq!(start.program, "docker");
    assert_eq!(start.args.first().map(String::as_str), Some("compose"));
    assert!(
        start.args.iter().all(|a| !a.contains(&password)),
        "{:?}",
        start.args
    );
    assert!(
        start
            .env
            .contains(&("MIKE_DB_PASSWORD".to_string(), password))
    );
    assert!(
        start
            .env
            .contains(&("MIKE_DB_PORT".to_string(), "6666".to_string()))
    );
    for call in docker.calls() {
        assert!(
            call.program == "docker" || call.program == "docker-compose",
            "unexpected program in {}",
            call.shown()
        );
    }
    Ok(())
}

#[tokio::test]
async fn up_twice_keeps_the_password_and_never_removes_data() -> anyhow::Result<()> {
    let home = home()?;
    let server = FakeServer::new(Reply::Applied(0));
    let first_docker = Docker::new(&["running|healthy"]);
    let first = up(&context(&first_docker, &server, &home.paths, free), None).await?;

    let second_docker = Docker::new(&["running|healthy"]);
    let second = up(&context(&second_docker, &server, &home.paths, taken), None).await?;

    assert_eq!(first.url, second.url);
    assert!(second.already_running);
    assert!(second.replaced_url.is_none());
    for call in second_docker.calls() {
        for banned in ["down", "rm", "-v", "--volumes", "prune"] {
            assert!(call.args.iter().all(|a| a != banned), "{}", call.shown());
        }
    }
    assert_eq!(read_config(&home.paths).matches("database_url").count(), 1);
    Ok(())
}

#[tokio::test]
async fn up_with_a_chosen_port_uses_it_in_the_url_and_for_compose() -> anyhow::Result<()> {
    let home = home()?;
    let docker = Docker::new(&["running|healthy"]);
    let server = FakeServer::new(Reply::Applied(0));
    let outcome = up(&context(&docker, &server, &home.paths, free), Some(6701)).await?;
    assert_eq!(outcome.port, 6701);
    assert!(
        outcome.url.ends_with("@127.0.0.1:6701/mike"),
        "{}",
        outcome.url
    );
    let start = docker
        .call_with("up")
        .ok_or_else(|| anyhow::anyhow!("no compose up call"))?;
    assert!(
        start
            .env
            .contains(&("MIKE_DB_PORT".to_string(), "6701".to_string()))
    );
    Ok(())
}

#[tokio::test]
async fn up_with_a_busy_port_names_the_port_and_starts_nothing() -> anyhow::Result<()> {
    let home = home()?;
    let docker = Docker::new(&["absent"]);
    let server = FakeServer::new(Reply::Applied(0));
    let result = up(&context(&docker, &server, &home.paths, taken), None).await;
    let Err(DbError::PortInUse { port }) = result else {
        anyhow::bail!("expected PortInUse, got {result:?}");
    };
    assert_eq!(port, 6666);
    assert!(docker.call_with("up").is_none(), "{:?}", docker.shown());
    assert!(!read_config(&home.paths).contains("database_url"));
    Ok(())
}

#[tokio::test]
async fn up_without_docker_or_with_a_stopped_daemon_says_which() -> anyhow::Result<()> {
    let home = home()?;
    let server = FakeServer::new(Reply::Applied(0));
    let mut docker = Docker::new(&["absent"]);
    docker.daemon = Daemon::Missing;
    let missing = up(&context(&docker, &server, &home.paths, free), None).await;
    assert!(
        matches!(missing, Err(DbError::DockerMissing)),
        "{missing:?}"
    );

    let mut docker = Docker::new(&["absent"]);
    docker.daemon = Daemon::Down;
    let down = up(&context(&docker, &server, &home.paths, free), None).await;
    let Err(DbError::DockerNotRunning { detail }) = down else {
        anyhow::bail!("expected DockerNotRunning, got {down:?}");
    };
    assert_eq!(detail, "Cannot connect to the Docker daemon");
    Ok(())
}

#[tokio::test]
async fn up_falls_back_to_the_standalone_docker_compose_and_reports_neither() -> anyhow::Result<()>
{
    let home = home()?;
    let server = FakeServer::new(Reply::Applied(0));
    let mut docker = Docker::new(&["running|healthy"]);
    docker.compose = ComposeCli::Standalone;
    up(&context(&docker, &server, &home.paths, free), None).await?;
    let start = docker
        .call_with("up")
        .ok_or_else(|| anyhow::anyhow!("no compose up call"))?;
    assert_eq!(start.program, "docker-compose");
    assert_eq!(start.args.first().map(String::as_str), Some("-f"));

    let mut docker = Docker::new(&["absent"]);
    docker.compose = ComposeCli::Neither;
    let result = up(&context(&docker, &server, &home.paths, free), Some(6702)).await;
    assert!(matches!(result, Err(DbError::ComposeMissing)), "{result:?}");
    Ok(())
}

#[tokio::test]
async fn up_reports_a_container_that_exits_with_its_log() -> anyhow::Result<()> {
    let home = home()?;
    let docker = Docker::new(&["exited|"]);
    let server = FakeServer::new(Reply::Applied(0));
    let result = up(&context(&docker, &server, &home.paths, free), None).await;
    let Err(DbError::NotHealthy { state, logs, .. }) = result else {
        anyhow::bail!("expected NotHealthy, got {result:?}");
    };
    assert_eq!(state, "exited");
    assert!(logs.contains("database system is ready"), "{logs}");
    assert!(lock(&server.urls).is_empty());
    Ok(())
}

#[tokio::test]
async fn up_gives_up_after_the_configured_number_of_health_polls() -> anyhow::Result<()> {
    let home = home()?;
    let docker = Docker::new(&["running|starting"]);
    let server = FakeServer::new(Reply::Applied(0));
    let result = up(&context(&docker, &server, &home.paths, free), None).await;
    assert!(
        matches!(result, Err(DbError::NotHealthy { .. })),
        "{result:?}"
    );
    let inspections = docker
        .shown()
        .iter()
        .filter(|s| s.contains("container inspect"))
        .count();
    assert_eq!(inspections, 3);
    Ok(())
}

#[tokio::test]
async fn up_surfaces_a_failing_compose_command_with_its_error_text() -> anyhow::Result<()> {
    let home = home()?;
    let mut docker = Docker::new(&["absent"]);
    docker.up_failure = Some("Bind for 127.0.0.1:6666 failed: port is already allocated");
    let server = FakeServer::new(Reply::Applied(0));
    let result = up(&context(&docker, &server, &home.paths, free), None).await;
    let Err(DbError::CommandFailed { detail, .. }) = result else {
        anyhow::bail!("expected CommandFailed, got {result:?}");
    };
    assert!(detail.contains("port is already allocated"), "{detail}");
    Ok(())
}

#[tokio::test]
async fn up_against_a_volume_with_another_password_saves_nothing() -> anyhow::Result<()> {
    let home = home()?;
    let docker = Docker::new(&["running|healthy"]);
    let server = FakeServer::new(Reply::Auth);
    let result = up(&context(&docker, &server, &home.paths, free), None).await;
    assert!(
        matches!(result, Err(DbError::PasswordMismatch)),
        "{result:?}"
    );
    assert!(!read_config(&home.paths).contains("database_url"));
    Ok(())
}

#[tokio::test]
async fn up_whose_migrations_fail_still_saves_the_url_and_says_so() -> anyhow::Result<()> {
    let home = home()?;
    let docker = Docker::new(&["running|healthy"]);
    let server = FakeServer::new(Reply::Other(
        "2 applied migrations are unknown to this build",
    ));
    let result = up(&context(&docker, &server, &home.paths, free), None).await;
    let Err(DbError::Migrate(why)) = result else {
        anyhow::bail!("expected Migrate, got {result:?}");
    };
    assert!(why.contains("unknown to this build"), "{why}");
    assert!(read_config(&home.paths).contains("postgres://mike:"));
    Ok(())
}

#[tokio::test]
async fn up_replaces_another_database_url_and_reports_it_without_the_password() -> anyhow::Result<()>
{
    let home = home()?;
    crate::db_provision::credentials::save_database_url(
        &home.paths.config_file,
        "postgres://ada:hunter2@db.example:5432/captures",
    )?;
    let docker = Docker::new(&["running|healthy"]);
    let server = FakeServer::new(Reply::Applied(0));
    let outcome = up(&context(&docker, &server, &home.paths, free), None).await?;
    assert_eq!(
        outcome.replaced_url.as_deref(),
        Some("postgres://ada:***@db.example:5432/captures")
    );
    assert!(!read_config(&home.paths).contains("hunter2"));
    Ok(())
}
