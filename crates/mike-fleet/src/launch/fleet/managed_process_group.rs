use std::io;
use std::time::Duration;

use crate::launch::fleet::owned_execution::{Owner, Phase};

#[path = "managed_process_census.rs"]
pub mod census;

pub struct ChildGroup {
    pub child: tokio::process::Child,
    group: Option<libc::pid_t>,
}

impl ChildGroup {
    pub fn new(child: tokio::process::Child) -> Result<Self, String> {
        let pid = child
            .id()
            .ok_or("spawned command has no process identity")?;
        let pid =
            libc::pid_t::try_from(pid).map_err(|_| "spawned process identity is out of range")?;
        Ok(Self {
            child,
            group: Some(pid),
        })
    }

    pub fn id(&self) -> Result<u32, String> {
        self.group
            .map(|pid| pid as u32)
            .ok_or("owned process identity was lost".into())
    }

    pub fn exited_unreaped(&mut self) -> Result<bool, String> {
        let pid = self
            .group
            .ok_or("owned group identity is no longer retained")?;
        use rustix::process::{Pid, WaitIdOptions};
        let options = WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT;
        let waited = Pid::from_raw(pid)
            .ok_or(rustix::io::Errno::INVAL)
            .and_then(|pid| rustix::process::waitid(rustix::process::WaitId::Pid(pid), options));
        match waited {
            Ok(status) => Ok(status.is_some()),
            Err(rustix::io::Errno::INTR) => Ok(false),
            Err(errno) => {
                self.group = None;
                Err(format!(
                    "owned child identity cannot be retained; group signalling refused: {}",
                    io::Error::from(errno)
                ))
            }
        }
    }

    pub async fn terminate_and_reap(
        &mut self,
        owner: Option<&Owner>,
    ) -> Result<Option<i32>, String> {
        self.exited_unreaped()?;
        let pid = self.group.ok_or("owned process group identity was lost")?;
        if let Err(error) = signal(pid)
            && !census::quiet(pid).await?
        {
            return Err(error);
        }
        if let Some(owner) = owner {
            owner.phase(Phase::Cleaning, None);
        }
        let mut delay = Duration::from_millis(50);
        loop {
            if census::quiet(pid).await? {
                break;
            }
            tokio::time::sleep(delay).await;
            delay = (delay * 2).min(Duration::from_secs(2));
        }
        self.group = None;
        self.child
            .wait()
            .await
            .map(|status| status.code())
            .map_err(|error| format!("owned child could not be reaped: {error}"))
    }
}

impl Drop for ChildGroup {
    fn drop(&mut self) {
        if let Some(pid) = self.group.take() {
            let _ = signal(pid);
        }
    }
}

fn signal(pid: libc::pid_t) -> Result<(), String> {
    let Some(group) = rustix::process::Pid::from_raw(pid) else {
        return Ok(());
    };
    match rustix::process::kill_process_group(group, rustix::process::Signal::KILL) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(errno) => Err(format!(
            "owned process group could not be stopped: {}",
            io::Error::from(errno)
        )),
    }
}
