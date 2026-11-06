use std::ffi::OsStr;
use std::time::Duration;

use anyhow::{Result, ensure};

use crate::config::paths::InstallationPaths;

use super::docker_command::DockerCommand;
use super::docker_resource::{self, Resources};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DockerReadiness {
    pub daemon: bool,
    pub compose: bool,
}

pub async fn probe() -> DockerReadiness {
    probe_local_with(DockerCommand::default()).await
}

async fn probe_local_with(docker: DockerCommand) -> DockerReadiness {
    match docker.pin_local_endpoint().await {
        Ok(docker) => probe_with(&docker).await,
        Err(_) => DockerReadiness {
            daemon: false,
            compose: false,
        },
    }
}

async fn probe_with(docker: &DockerCommand) -> DockerReadiness {
    let daemon = docker
        .run(
            &["info", "--format", "{{.ServerVersion}}"],
            Duration::from_secs(3),
        )
        .await
        .is_ok();
    let compose = docker
        .run(&["compose", "version", "--short"], Duration::from_secs(3))
        .await
        .is_ok();
    DockerReadiness { daemon, compose }
}

pub async fn provision(
    paths: &InstallationPaths,
    port: u16,
    image: Option<&str>,
) -> Result<String> {
    ensure!(port != 0, "choose a nonzero database port");
    let image = docker_resource::image(image)?;
    let docker = require_docker(DockerCommand::default()).await?;
    let resources = Resources::new(paths);
    let _lock = super::credential::lock(&resources.directory)?;
    let existing = resources.check_ownership(&docker, Some(image)).await?;
    if !existing {
        let address = std::net::SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, port);
        let _available = std::net::TcpListener::bind(address)
            .map_err(|_| anyhow::anyhow!("selected database port is occupied or unavailable; choose another loopback port and retry"))?;
    }
    resources.materialize()?;
    let password = super::credential::load_or_create(&resources.credential, existing)?;
    if image == docker_resource::DEFAULT_IMAGE {
        compose(&docker, &resources, port, image, &["build"]).await?;
    } else {
        docker
            .run(&["pull", image], Duration::from_secs(600))
            .await?;
    }
    compose(
        &docker,
        &resources,
        port,
        image,
        &["up", "--detach", "--wait", "--wait-timeout", "60"],
    )
    .await?;
    let database_url = format!("postgres://mike:{password}@127.0.0.1:{port}/mike");
    super::database::await_managed(&database_url).await?;
    Ok(database_url)
}

pub async fn stop(paths: &InstallationPaths) -> Result<()> {
    let docker = require_docker(DockerCommand::default()).await?;
    let resources = Resources::new(paths);
    let _lock = super::credential::lock(&resources.directory)?;
    if !resources.check_ownership(&docker, None).await? {
        return Ok(());
    }
    resources.materialize()?;
    compose(
        &docker,
        &resources,
        6666,
        docker_resource::DEFAULT_IMAGE,
        &["stop", "--timeout", "30"],
    )
    .await?;
    Ok(())
}

async fn require_docker(docker: DockerCommand) -> Result<DockerCommand> {
    let docker = docker.pin_local_endpoint().await?;
    let readiness = probe_with(&docker).await;
    ensure!(
        readiness.daemon,
        "Docker daemon is unavailable; start Docker or choose an existing PostgreSQL database"
    );
    ensure!(
        readiness.compose,
        "Docker Compose plugin is unavailable; install Docker Compose or choose an existing PostgreSQL database"
    );
    Ok(docker)
}

async fn compose(
    docker: &DockerCommand,
    resources: &Resources,
    port: u16,
    image: &str,
    action: &[&str],
) -> Result<()> {
    let port = port.to_string();
    let environment = [
        ("MIKE_INSTALLATION_ID", OsStr::new(&resources.identity)),
        ("MIKE_DATABASE_NAME", OsStr::new(&resources.name)),
        ("MIKE_DATABASE_VOLUME", OsStr::new(&resources.volume)),
        ("MIKE_DATABASE_IMAGE", OsStr::new(image)),
        ("MIKE_DATABASE_PORT", OsStr::new(&port)),
        (
            "MIKE_DATABASE_PASSWORD_FILE",
            resources.credential.as_os_str(),
        ),
    ];
    let mut args = vec![
        "compose",
        "--env-file",
        "compose.env",
        "--project-name",
        &resources.project,
        "--file",
        "compose.yml",
    ];
    args.extend(action);
    docker
        .execute(
            &args,
            &environment,
            Some(&resources.directory),
            Duration::from_secs(600),
        )
        .await?;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn compose_is_checked_separately_from_daemon() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let script = dir.path().join("docker");
        std::fs::write(&script, "#!/bin/sh\n[ \"$1\" = info ]\n")?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))?;
        let readiness = probe_local_with(DockerCommand::fixture_with_selection(
            &script,
            Some("unix:///local.sock"),
            None,
        ))
        .await;
        assert!(readiness.daemon);
        assert!(!readiness.compose);
        Ok(())
    }

    #[tokio::test]
    async fn remote_context_is_refused_before_readiness_or_resource_operations() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let script = dir.path().join("docker");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"$(dirname \"$0\")/calls\"\nprintf '\"tcp://remote:2375\"\\n'\n",
        )?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))?;
        let docker = DockerCommand::fixture_with_selection(
            &script,
            Some("unix:///local.sock"),
            Some("remote"),
        );
        assert!(require_docker(docker).await.is_err());
        let readiness = probe_local_with(DockerCommand::fixture_with_selection(
            &script,
            Some("unix:///local.sock"),
            Some("remote"),
        ))
        .await;
        assert_eq!(
            readiness,
            DockerReadiness {
                daemon: false,
                compose: false,
            }
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("calls"))?,
            "context\ncontext\n"
        );
        Ok(())
    }

    #[tokio::test]
    async fn readiness_and_resource_commands_use_the_same_pinned_endpoint() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let script = dir.path().join("docker");
        std::fs::write(
            &script,
            r#"#!/bin/sh
set -eu
if [ "$1" = context ]; then
    printf '"unix:///selected.sock"\n'
else
    [ "$DOCKER_HOST" = "unix:///selected.sock" ]
    [ "${DOCKER_CONTEXT-unset}" = unset ]
    printf '%s\n' "$1" >> "$(dirname "$0")/calls"
fi
"#,
        )?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))?;
        let docker = require_docker(DockerCommand::fixture_with_selection(
            &script,
            Some("tcp://ignored:2375"),
            Some("selected"),
        ))
        .await?;
        let paths = InstallationPaths::resolve(Some(&dir.path().join("config.toml")))?;
        assert!(
            !Resources::new(&paths)
                .check_ownership(&docker, None)
                .await?
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("calls"))?,
            "info\ncompose\ncontainer\nvolume\n"
        );
        Ok(())
    }

    #[tokio::test]
    async fn zero_port_is_rejected_before_using_docker() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let paths = InstallationPaths::resolve(Some(&dir.path().join("config.toml")))?;
        assert!(provision(&paths, 0, None).await.is_err());
        assert!(!paths.state_dir.exists());
        Ok(())
    }
}
