use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result};

use crate::config::ProxyConfig;

use super::instance::InstanceSpec;

pub(crate) struct LaunchedChild {
    child: Child,
    snapshot: PathBuf,
    released: bool,
    reaped: bool,
}

impl LaunchedChild {
    pub(crate) fn spawn(config: &ProxyConfig, log_path: &Path) -> Result<Self> {
        let exe =
            std::env::current_exe().context("cannot resolve the installed mike executable")?;
        let log = private_log(log_path)?;
        let stderr = log
            .try_clone()
            .context("cannot duplicate private daemon log")?;
        let mut command = Command::new(exe);
        let snapshot = crate::config::configure_child(&mut command, config)?;
        command
            .arg("serve")
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr));
        detach(&mut command);
        let child = command.spawn().map_err(|error| {
            let _ = std::fs::remove_file(&snapshot);
            anyhow::anyhow!("cannot launch the selected installation: {error}")
        })?;
        Ok(Self {
            child,
            snapshot,
            released: false,
            reaped: false,
        })
    }

    pub(crate) fn pid(&self) -> u32 {
        self.child.id()
    }

    pub(crate) fn exited(&mut self) -> Result<bool> {
        if self.reaped {
            return Ok(true);
        }
        self.reaped = self
            .child
            .try_wait()
            .context("cannot inspect newly launched daemon")?
            .is_some();
        Ok(self.reaped)
    }

    pub(crate) fn release(mut self) {
        self.released = true;
    }

    pub(crate) async fn cleanup(&mut self, spec: &InstanceSpec) {
        if self.exited().unwrap_or(true) {
            return;
        }
        if let Ok(Some(status)) = super::control::status(spec, false).await
            && status.identity.pid == self.pid()
        {
            let _ = super::control::stop(spec, &status.identity).await;
            let started = std::time::Instant::now();
            while started.elapsed() < Duration::from_secs(5) {
                if self.exited().unwrap_or(true) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        self.kill_unreaped_child();
    }

    fn kill_unreaped_child(&mut self) {
        if self.reaped || self.released {
            return;
        }
        match self.child.try_wait() {
            Ok(Some(_)) => self.reaped = true,
            Ok(None) => {
                if self.child.kill().is_ok() {
                    self.reaped = self.child.wait().is_ok();
                }
            }
            Err(_) => {}
        }
    }
}

impl Drop for LaunchedChild {
    fn drop(&mut self) {
        self.kill_unreaped_child();
        let _ = std::fs::remove_file(&self.snapshot);
    }
}

fn private_log(path: &Path) -> Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = options
        .open(path)
        .context("cannot open private daemon log")?;
    let metadata = file
        .metadata()
        .context("cannot inspect private daemon log")?;
    anyhow::ensure!(metadata.is_file(), "daemon log must be a regular file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        anyhow::ensure!(
            metadata.uid() == rustix::process::geteuid().as_raw()
                && metadata.permissions().mode() & 0o077 == 0,
            "daemon log must be private and owned by this user"
        );
    }
    Ok(file)
}

#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn detach(_command: &mut Command) {}
