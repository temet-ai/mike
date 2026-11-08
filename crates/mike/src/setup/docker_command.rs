use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Output;
use std::time::Duration;

use anyhow::{Result, anyhow, bail, ensure};

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
        let host = match given_host(self.host.as_deref(), self.context.as_deref())? {
            Some(host) => host,
            None => context_endpoint(
                &self
                    .run(
                        &[
                            "context",
                            "inspect",
                            "--format",
                            "{{json .Endpoints.docker.Host}}",
                        ],
                        Duration::from_secs(3),
                    )
                    .await?,
            )?,
        };
        require_local(&host)?;
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
        let mut command = self.command(args, environment, directory);
        outcome(tokio::time::timeout(deadline, command.output()).await.ok())
    }

    fn command(
        &self,
        args: &[&str],
        environment: &[(&str, &OsStr)],
        directory: Option<&Path>,
    ) -> tokio::process::Command {
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
        command
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

fn given_host(host: Option<&OsStr>, context: Option<&OsStr>) -> Result<Option<String>> {
    match host.filter(|host| !host.is_empty()) {
        Some(host) if context.is_none_or(OsStr::is_empty) => host
            .to_str()
            .map(|host| Some(host.to_owned()))
            .ok_or_else(|| anyhow!("cannot verify the selected Docker host")),
        _ => Ok(None),
    }
}

fn context_endpoint(output: &[u8]) -> Result<String> {
    serde_json::from_slice::<String>(output)
        .map_err(|_| anyhow!("cannot verify the selected Docker context"))
}

fn require_local(host: &str) -> Result<()> {
    ensure!(
        host.starts_with("unix://") || host.starts_with("npipe://"),
        "managed setup requires a local Docker socket; select a local Docker context or use an existing database URL"
    );
    Ok(())
}

fn outcome(finished: Option<std::io::Result<Output>>) -> Result<Vec<u8>> {
    match finished {
        Some(Ok(output)) if output.status.success() => Ok(output.stdout),
        Some(Ok(_)) => bail!(
            "Docker command failed; check the Docker daemon, image access, resource ownership and selected port"
        ),
        Some(Err(_)) => bail!(
            "Docker could not be launched; install Docker and its Compose plugin or choose an existing database"
        ),
        None => bail!(
            "Docker operation exceeded its deadline; inspect the owned resources and retry; no data was removed"
        ),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::ExitStatusExt;

    const NOT_INSTALLED: &str = "/nonexistent/docker";

    fn host(value: &str) -> Option<&OsStr> {
        Some(OsStr::new(value))
    }

    #[test]
    fn explicit_host_without_context_is_used_directly() -> Result<()> {
        for context in [None, host("")] {
            assert_eq!(
                given_host(host("unix:///selected.sock"), context)?,
                Some("unix:///selected.sock".to_owned())
            );
        }
        Ok(())
    }

    #[test]
    fn selected_context_or_absent_host_resolves_through_the_context() -> Result<()> {
        assert_eq!(
            given_host(host("unix:///ignored.sock"), host("remote"))?,
            None
        );
        for absent in [None, host("")] {
            for context in [None, host(""), host("selected")] {
                assert_eq!(given_host(absent, context)?, None);
            }
        }
        Ok(())
    }

    #[test]
    fn only_local_sockets_are_accepted_and_refusals_hide_the_address() -> Result<()> {
        require_local("unix:///var/run/docker.sock")?;
        require_local("npipe:////./pipe/docker_engine")?;
        for remote in [
            "tcp://private:secret@remote:2375",
            "ssh://private:secret@remote",
        ] {
            let error = require_local(remote)
                .err()
                .ok_or_else(|| anyhow!("{remote} was accepted"))?
                .to_string();
            assert!(error.contains("local Docker socket"));
            assert!(!error.contains("private") && !error.contains("secret"));
        }
        Ok(())
    }

    #[test]
    fn context_metadata_must_be_a_json_string_and_refusals_hide_it() -> Result<()> {
        assert_eq!(
            context_endpoint(b"\"unix:///current.sock\"\n")?,
            "unix:///current.sock"
        );
        let error = context_endpoint(b"private-secret-context\n")
            .err()
            .ok_or_else(|| anyhow!("invalid context metadata was accepted"))?
            .to_string();
        assert!(error.contains("cannot verify"));
        assert!(!error.contains("private") && !error.contains("secret"));
        Ok(())
    }

    #[tokio::test]
    async fn remote_host_is_refused_before_docker_is_run() -> Result<()> {
        let error = DockerCommand::fixture_with_selection(
            Path::new(NOT_INSTALLED),
            Some("tcp://private:secret@remote:2375"),
            None,
        )
        .pin_local_endpoint()
        .await
        .err()
        .ok_or_else(|| anyhow!("remote host was accepted"))?
        .to_string();
        assert!(error.contains("local Docker socket"));
        Ok(())
    }

    #[tokio::test]
    async fn pinned_endpoint_overrides_any_docker_selection_passed_to_a_command() -> Result<()> {
        let docker = DockerCommand::fixture_with_selection(
            Path::new(NOT_INSTALLED),
            Some("unix:///selected.sock"),
            Some(""),
        )
        .pin_local_endpoint()
        .await?
        .pin_local_endpoint()
        .await?;
        let command = docker.command(
            &["compose", "up"],
            &[
                ("DOCKER_HOST", OsStr::new("tcp://redirected:2375")),
                ("DOCKER_CONTEXT", OsStr::new("redirected")),
            ],
            None,
        );
        let env: BTreeMap<_, _> = command.as_std().get_envs().collect();
        assert_eq!(
            env.get(OsStr::new("DOCKER_HOST")),
            Some(&Some(OsStr::new("unix:///selected.sock")))
        );
        assert_eq!(env.get(OsStr::new("DOCKER_CONTEXT")), Some(&None));
        Ok(())
    }

    #[test]
    fn failures_never_repeat_what_docker_printed() -> Result<()> {
        let leak = b"postgres://private:secret@host/db".to_vec();
        let failed = Output {
            status: std::process::ExitStatus::from_raw(1 << 8),
            stdout: leak.clone(),
            stderr: leak,
        };
        let launch = std::io::Error::other("postgres://private:secret@host/db");
        for finished in [Some(Ok(failed)), Some(Err(launch)), None] {
            let error = outcome(finished)
                .err()
                .ok_or_else(|| anyhow!("failure was accepted"))?
                .to_string();
            assert!(!error.contains("secret") && !error.contains("postgres://"));
        }
        let done = Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: b"info".to_vec(),
            stderr: Vec::new(),
        };
        assert_eq!(outcome(Some(Ok(done)))?, b"info");
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
            .ok_or_else(|| anyhow!("deadline"))?;
        assert!(error.to_string().contains("deadline"));
        Ok(())
    }
}
