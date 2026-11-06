use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::time::Duration;

use anyhow::{Result, bail, ensure};

pub(super) struct DockerCommand {
    executable: OsString,
    host: Option<OsString>,
    context: Option<OsString>,
}

impl Default for DockerCommand {
    fn default() -> Self {
        Self {
            executable: "docker".into(),
            host: std::env::var_os("DOCKER_HOST"),
            context: std::env::var_os("DOCKER_CONTEXT"),
        }
    }
}

impl DockerCommand {
    pub(super) async fn pin_local_endpoint(mut self) -> Result<Self> {
        let host = match self.host.as_ref().filter(|host| !host.is_empty()) {
            Some(host)
                if self
                    .context
                    .as_ref()
                    .is_none_or(|context| context.is_empty()) =>
            {
                host.to_str()
                    .ok_or_else(|| anyhow::anyhow!("cannot verify the selected Docker host"))?
                    .to_owned()
            }
            _ => {
                let output = self
                    .run(
                        &[
                            "context",
                            "inspect",
                            "--format",
                            "{{json .Endpoints.docker.Host}}",
                        ],
                        Duration::from_secs(3),
                    )
                    .await?;
                serde_json::from_slice::<String>(&output)
                    .map_err(|_| anyhow::anyhow!("cannot verify the selected Docker context"))?
            }
        };
        ensure!(
            host.starts_with("unix://") || host.starts_with("npipe://"),
            "managed setup requires a local Docker socket; select a local Docker context or use an existing database URL"
        );
        self.host = Some(host.into());
        self.context = None;
        Ok(self)
    }

    pub(super) async fn run(&self, args: &[&str], deadline: Duration) -> Result<Vec<u8>> {
        self.execute(args, &[], None, deadline).await
    }

    pub(super) async fn execute(
        &self,
        args: &[&str],
        environment: &[(&str, &OsStr)],
        directory: Option<&Path>,
        deadline: Duration,
    ) -> Result<Vec<u8>> {
        let mut command = tokio::process::Command::new(&self.executable);
        command
            .args(args)
            .kill_on_drop(true)
            .stdin(std::process::Stdio::null());
        for (key, value) in environment {
            command.env(key, value);
        }
        for (key, value) in [
            ("DOCKER_HOST", self.host.as_ref()),
            ("DOCKER_CONTEXT", self.context.as_ref()),
        ] {
            match value {
                Some(value) => command.env(key, value),
                None => command.env_remove(key),
            };
        }
        if let Some(directory) = directory {
            command.current_dir(directory);
        }
        match tokio::time::timeout(deadline, command.output()).await {
            Ok(Ok(output)) if output.status.success() => Ok(output.stdout),
            Ok(Ok(_)) => bail!(
                "Docker command failed; check the Docker daemon, image access, resource ownership and selected port"
            ),
            Ok(Err(_)) => bail!(
                "Docker could not be launched; install Docker and its Compose plugin or choose an existing database"
            ),
            Err(_) => bail!(
                "Docker operation exceeded its deadline; inspect the owned resources and retry; no data was removed"
            ),
        }
    }

    #[cfg(test)]
    pub(super) fn fixture(executable: &Path) -> Self {
        Self {
            executable: executable.as_os_str().to_owned(),
            host: None,
            context: None,
        }
    }

    #[cfg(test)]
    pub(super) fn fixture_with_selection(
        executable: &Path,
        host: Option<&str>,
        context: Option<&str>,
    ) -> Self {
        Self {
            executable: executable.as_os_str().to_owned(),
            host: host.map(OsString::from),
            context: context.map(OsString::from),
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn write_fixture(directory: &Path, body: &str) -> Result<std::path::PathBuf> {
        let script = directory.join("docker");
        std::fs::write(&script, format!("#!/bin/sh\nset -eu\n{body}\n"))?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))?;
        Ok(script)
    }

    #[tokio::test]
    async fn remote_context_overrides_local_host_and_is_refused() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let script = write_fixture(
            dir.path(),
            r#"[ "$1 $2" = 'context inspect' ]
[ "$DOCKER_HOST" = "unix:///ignored.sock" ]
[ "$DOCKER_CONTEXT" = remote ]
printf '"ssh://private:secret@remote"\n'"#,
        )?;
        let error = DockerCommand::fixture_with_selection(
            &script,
            Some("unix:///ignored.sock"),
            Some("remote"),
        )
        .pin_local_endpoint()
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("remote context was accepted"))?;
        assert!(error.to_string().contains("local Docker socket"));
        assert!(!error.to_string().contains("private"));
        assert!(!error.to_string().contains("secret"));
        Ok(())
    }

    #[tokio::test]
    async fn pinned_context_cannot_be_redirected_by_config_or_command_environment() -> Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("endpoint"), "\"unix:///selected.sock\"\n")?;
        let script = write_fixture(
            dir.path(),
            r#"directory=$(dirname "$0")
if [ "$1" = context ]; then
    [ "$DOCKER_HOST" = "tcp://ignored:2375" ]
    [ "$DOCKER_CONTEXT" = selected ]
    printf 'resolved\n' >> "$directory/resolutions"
    cat "$directory/endpoint"
else
    printf '%s|%s\n' "$DOCKER_HOST" "${DOCKER_CONTEXT-unset}"
fi"#,
        )?;
        let docker = DockerCommand::fixture_with_selection(
            &script,
            Some("tcp://ignored:2375"),
            Some("selected"),
        )
        .pin_local_endpoint()
        .await?;
        std::fs::write(dir.path().join("endpoint"), "\"tcp://redirected:2375\"\n")?;
        let docker = docker.pin_local_endpoint().await?;
        for args in [
            &["info"][..],
            &["container", "inspect", "owned"],
            &["volume", "inspect", "owned"],
            &["compose", "up"],
            &["compose", "stop"],
        ] {
            let output = docker
                .execute(
                    args,
                    &[
                        ("DOCKER_HOST", OsStr::new("tcp://redirected:2375")),
                        ("DOCKER_CONTEXT", OsStr::new("redirected")),
                    ],
                    Some(dir.path()),
                    Duration::from_secs(3),
                )
                .await?;
            assert_eq!(output, b"unix:///selected.sock|unset\n");
        }
        assert_eq!(
            std::fs::read_to_string(dir.path().join("resolutions"))?,
            "resolved\n"
        );
        Ok(())
    }

    #[tokio::test]
    async fn explicit_host_with_empty_context_skips_context_resolution() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let script = write_fixture(
            dir.path(),
            r#"[ "$1" != context ]
printf '%s|%s\n' "$DOCKER_HOST" "${DOCKER_CONTEXT-unset}""#,
        )?;
        for host in ["unix:///selected.sock", "npipe:////./pipe/docker_engine"] {
            for context in [None, Some("")] {
                let docker = DockerCommand::fixture_with_selection(&script, Some(host), context)
                    .pin_local_endpoint()
                    .await?;
                let output = docker.run(&["info"], Duration::from_secs(3)).await?;
                assert_eq!(output, format!("{host}|unset\n").as_bytes());
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn absent_or_empty_host_resolves_the_current_context() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let script = write_fixture(
            dir.path(),
            r#"if [ "$1" = context ]; then
    printf '"unix:///current.sock"\n'
else
    printf '%s|%s\n' "$DOCKER_HOST" "${DOCKER_CONTEXT-unset}"
fi"#,
        )?;
        for host in [None, Some("")] {
            for context in [None, Some("")] {
                let docker = DockerCommand::fixture_with_selection(&script, host, context)
                    .pin_local_endpoint()
                    .await?;
                let output = docker.run(&["info"], Duration::from_secs(3)).await?;
                assert_eq!(output, b"unix:///current.sock|unset\n");
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn direct_remote_host_is_refused_without_running_docker() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let script = write_fixture(dir.path(), "touch \"$(dirname \"$0\")/called\"")?;
        let error = DockerCommand::fixture_with_selection(
            &script,
            Some("tcp://private:secret@remote:2375"),
            None,
        )
        .pin_local_endpoint()
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("remote host was accepted"))?;
        assert!(error.to_string().contains("local Docker socket"));
        assert!(!error.to_string().contains("secret"));
        assert!(!dir.path().join("called").exists());
        Ok(())
    }

    #[tokio::test]
    async fn invalid_context_metadata_is_private_and_refused() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let script = write_fixture(dir.path(), "printf 'private-secret-context\\n'")?;
        let error = DockerCommand::fixture(&script)
            .pin_local_endpoint()
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("invalid context metadata was accepted"))?;
        assert!(error.to_string().contains("cannot verify"));
        assert!(!error.to_string().contains("private"));
        assert!(!error.to_string().contains("secret"));
        Ok(())
    }

    #[tokio::test]
    async fn command_failures_hide_child_output() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let script = dir.path().join("docker");
        std::fs::write(
            &script,
            "#!/bin/sh\necho postgres://private:secret@host/db >&2\nexit 1\n",
        )?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))?;
        let error = DockerCommand::fixture(&script)
            .run(&["info"], Duration::from_secs(3))
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("fixture failure"))?;
        assert!(!error.to_string().contains("secret"));
        assert!(!error.to_string().contains("postgres://"));
        Ok(())
    }

    #[tokio::test]
    async fn deadline_kills_a_stalled_child() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let script = dir.path().join("docker");
        std::fs::write(&script, "#!/bin/sh\nexec sleep 10\n")?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))?;
        let error = DockerCommand::fixture(&script)
            .run(&[], Duration::from_millis(30))
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("deadline"))?;
        assert!(error.to_string().contains("deadline"));
        Ok(())
    }
}
