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
        let mut status = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                status.as_mut_ptr(),
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EINTR) {
                return Ok(false);
            }
            self.group = None;
            return Err(format!(
                "owned child identity cannot be retained; group signalling refused: {error}"
            ));
        }
        Ok(unsafe { status.assume_init().si_pid() } != 0)
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
    let result = unsafe { libc::kill(-pid, libc::SIGKILL) };
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(format!("owned process group could not be stopped: {error}"))
    }
}
