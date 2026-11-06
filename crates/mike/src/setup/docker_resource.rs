use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, ensure};

use crate::config::paths::InstallationPaths;

use super::docker_command::DockerCommand;

pub(super) const DEFAULT_IMAGE: &str = "mike/postgres-pgvector:19rc1-0.8.7";
pub(crate) const DOCKERFILE: &str = include_str!("../../resources/postgres19.Dockerfile");
const LABEL: &str = "io.temet.mike.installation";
const MANAGED: &str = "io.temet.mike.managed-database";
const RESOURCE: &str = include_str!("../../resources/local-db.compose.yml");
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) struct Resources {
    pub identity: String,
    pub project: String,
    pub name: String,
    pub volume: String,
    pub directory: PathBuf,
    pub credential: PathBuf,
}

impl Resources {
    pub(super) fn new(paths: &InstallationPaths) -> Self {
        let project = format!("mike-{}", paths.installation_id);
        let directory = paths.state_dir.join("database");
        Self {
            identity: paths.installation_id.clone(),
            name: format!("{project}-database"),
            volume: format!("{project}-data"),
            project,
            credential: directory.join("password"),
            directory,
        }
    }

    pub(super) fn materialize(&self) -> Result<()> {
        super::credential::prepare_directory(&self.directory)?;
        crate::owner_only_file::write(&self.directory.join("compose.yml"), RESOURCE)
            .context("cannot materialize the embedded database resource")?;
        crate::owner_only_file::write(&self.directory.join("Dockerfile.pg19"), DOCKERFILE)
            .context("cannot materialize the PostgreSQL 19 build resource")?;
        crate::owner_only_file::write(&self.directory.join("compose.env"), "")
            .context("cannot isolate Compose from checkout environment files")?;
        Ok(())
    }

    pub(super) async fn check_ownership(
        &self,
        docker: &DockerCommand,
        image: Option<&str>,
    ) -> Result<bool> {
        let container = self.inspect(docker, "container", &self.name).await?;
        let volume = self.inspect(docker, "volume", &self.volume).await?;
        if let Some(container) = &container {
            validate_labels(container.pointer("/Config/Labels"), &self.identity)?;
            ensure!(
                container
                    .pointer("/Config/Labels/com.docker.compose.project")
                    .and_then(|v| v.as_str())
                    == Some(self.project.as_str()),
                "database container is not owned by this Compose project; refusing to adopt it"
            );
            validate_container_mounts(container, &self.volume)?;
            if let Some(image) = image {
                ensure!(
                    container.pointer("/Config/Image").and_then(|v| v.as_str()) == Some(image),
                    "managed database image differs from the selected PostgreSQL 19 image; use an explicit reviewed database upgrade rather than recreating it during setup"
                );
            }
        }
        if let Some(volume) = &volume {
            validate_labels(volume.get("Labels"), &self.identity)?;
        }
        ensure!(
            container.is_none() || volume.is_some(),
            "managed container exists without its owned volume; inspect the resources before retrying"
        );
        Ok(container.is_some() || volume.is_some())
    }

    async fn inspect(
        &self,
        docker: &DockerCommand,
        kind: &str,
        name: &str,
    ) -> Result<Option<serde_json::Value>> {
        let filter = if kind == "container" {
            format!("name=^/{name}$")
        } else {
            format!("name=^{name}$")
        };
        let mut args = vec![kind, "ls", "--filter", &filter, "--format", "{{.Name}}"];
        if kind == "container" {
            args = vec![
                kind,
                "ls",
                "--all",
                "--filter",
                &filter,
                "--format",
                "{{.Names}}",
            ];
        }
        let output = docker.run(&args, COMMAND_TIMEOUT).await?;
        let listing =
            String::from_utf8(output).context("Docker returned invalid resource metadata")?;
        if !listing.lines().any(|line| line == name) {
            return Ok(None);
        }
        let output = docker
            .run(&[kind, "inspect", name], COMMAND_TIMEOUT)
            .await?;
        let values: Vec<serde_json::Value> = serde_json::from_slice(&output)
            .context("Docker returned invalid ownership metadata")?;
        ensure!(
            values.len() == 1,
            "Docker did not return exactly one selected resource"
        );
        Ok(values.into_iter().next())
    }
}

fn validate_container_mounts(container: &serde_json::Value, volume: &str) -> Result<()> {
    let owned_mount = container
        .get("Mounts")
        .and_then(|value| value.as_array())
        .is_some_and(|mounts| {
            mounts.iter().any(|mount| {
                mount.get("Name").and_then(|value| value.as_str()) == Some(volume)
                    && mount.get("Destination").and_then(|value| value.as_str())
                        == Some("/var/lib/postgresql/data")
            })
        });
    ensure!(
        owned_mount,
        "managed database container does not use its expected owned volume; inspect the resources before retrying"
    );
    let bindings = container
        .pointer("/HostConfig/PortBindings/5432~1tcp")
        .and_then(|value| value.as_array());
    ensure!(
        bindings.is_some_and(|ports| !ports.is_empty()
            && ports.iter().all(
                |port| port.get("HostIp").and_then(|value| value.as_str()) == Some("127.0.0.1")
            )),
        "managed database container has a non-loopback or missing port publication; inspect it before retrying"
    );
    Ok(())
}

fn validate_labels(labels: Option<&serde_json::Value>, identity: &str) -> Result<()> {
    let matches = labels.is_some_and(|labels| {
        labels.get(LABEL).and_then(|v| v.as_str()) == Some(identity)
            && labels.get(MANAGED).and_then(|v| v.as_str()) == Some("1")
    });
    ensure!(
        matches,
        "database resource name belongs to an unlabeled or foreign installation; refusing to adopt, modify or remove it"
    );
    Ok(())
}

pub(super) fn image(selected: Option<&str>) -> Result<&str> {
    let image = selected.unwrap_or(DEFAULT_IMAGE);
    if image == DEFAULT_IMAGE {
        return Ok(image);
    }
    let digest = image
        .strip_prefix("pgvector/pgvector@sha256:")
        .context("managed database image must be pgvector/pgvector pinned by sha256 digest")?;
    ensure!(
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "managed database image must contain a complete SHA-256 digest; mutable tags are not accepted"
    );
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_requires_an_official_digest() -> Result<()> {
        image(None)?;
        assert!(image(Some("pgvector/pgvector:latest")).is_err());
        assert!(image(Some("foreign/pgvector@sha256:1234")).is_err());
        Ok(())
    }

    #[test]
    fn ownership_rejects_foreign_and_unlabeled_resources() -> Result<()> {
        assert!(validate_labels(None, "mine").is_err());
        assert!(
            validate_labels(
                Some(&serde_json::json!({(LABEL): "other", (MANAGED): "1"})),
                "mine"
            )
            .is_err()
        );
        validate_labels(
            Some(&serde_json::json!({(LABEL): "mine", (MANAGED): "1"})),
            "mine",
        )
    }

    #[test]
    fn embedded_resource_is_materialized_outside_the_checkout() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().canonicalize()?;
        let paths = InstallationPaths::resolve(Some(&root.join("config.toml")))?;
        let resources = Resources::new(&paths);
        resources.materialize()?;
        assert_eq!(
            std::fs::read_to_string(resources.directory.join("compose.yml"))?,
            RESOURCE
        );
        assert!(RESOURCE.contains("127.0.0.1:"));
        assert!(RESOURCE.contains("POSTGRES_PASSWORD_FILE"));
        assert!(!RESOURCE.contains("POSTGRES_PASSWORD:"));
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod fixture_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn absent_resources_are_distinct_from_docker_errors() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let paths = InstallationPaths::resolve(Some(&dir.path().join("config.toml")))?;
        let resources = Resources::new(&paths);
        let script = dir.path().join("docker");
        std::fs::write(&script, "#!/bin/sh\nexit 0\n")?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))?;
        let docker = DockerCommand::fixture(&script);
        assert!(!resources.check_ownership(&docker, None).await?);
        std::fs::write(&script, "#!/bin/sh\nexit 1\n")?;
        assert!(resources.check_ownership(&docker, None).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn exact_name_collision_cannot_adopt_an_unlabeled_volume() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let paths = InstallationPaths::resolve(Some(&dir.path().join("config.toml")))?;
        let resources = Resources::new(&paths);
        let script = dir.path().join("docker");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\ncase \"$1 $2\" in\n'volume ls') echo '{}' ;;\n'volume inspect') echo '[{{\"Labels\":{{}}}}]' ;;\nesac\n",
                resources.volume
            ),
        )?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))?;
        let error = resources
            .check_ownership(&DockerCommand::fixture(&script), None)
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("foreign volume"))?;
        assert!(error.to_string().contains("foreign"));
        assert!(!resources.credential.exists());
        Ok(())
    }

    #[test]
    fn owned_labels_do_not_authorize_foreign_storage_or_public_ports() {
        let foreign = serde_json::json!({"Mounts":[{"Name":"foreign","Destination":"/var/lib/postgresql/data"}]});
        assert!(validate_container_mounts(&foreign, "mine").is_err());
        let public = serde_json::json!({"Mounts":[{"Name":"mine","Destination":"/var/lib/postgresql/data"}],"HostConfig":{"PortBindings":{"5432/tcp":[{"HostIp":"0.0.0.0","HostPort":"6666"}]}}});
        assert!(validate_container_mounts(&public, "mine").is_err());
    }
}
