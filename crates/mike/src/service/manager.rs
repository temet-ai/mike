use std::path::{Path, PathBuf};

use super::error::ServiceError;
use super::platform::Platform;
use super::spec::{ServiceKind, Spec};
use super::state::{self, Activity};
use super::{launchd, systemd};
use crate::db_provision::{Call, Captured};

#[derive(Debug, Clone)]
pub(crate) struct Manager {
    pub(crate) platform: Platform,
    unit_dir: PathBuf,
    uid: u32,
}

fn current_uid() -> u32 {
    unsafe { libc::getuid() }
}

fn unit_dir_for(platform: Platform, home: &Path, xdg_config_home: Option<&str>) -> PathBuf {
    match platform {
        Platform::Launchd => home.join("Library").join("LaunchAgents"),
        Platform::Systemd => {
            let base = xdg_config_home
                .filter(|v| Path::new(v).is_absolute())
                .map_or_else(|| home.join(".config"), PathBuf::from);
            base.join("systemd").join("user")
        }
    }
}

impl Manager {
    pub(crate) fn new(
        platform: Platform,
        home: &Path,
        xdg_config_home: Option<&str>,
        uid: u32,
    ) -> Self {
        Self {
            platform,
            unit_dir: unit_dir_for(platform, home, xdg_config_home),
            uid,
        }
    }

    pub(crate) fn from_process() -> Result<Self, ServiceError> {
        let platform = Platform::current().ok_or_else(|| ServiceError::UnsupportedPlatform {
            os: std::env::consts::OS.to_string(),
        })?;
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
            .ok_or(ServiceError::NoHome)?;
        Ok(Self::new(
            platform,
            &home,
            std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
            current_uid(),
        ))
    }

    pub(crate) fn identifier(&self, kind: ServiceKind) -> &'static str {
        match self.platform {
            Platform::Launchd => kind.launchd_label(),
            Platform::Systemd => kind.systemd_unit(),
        }
    }

    pub(crate) fn file_path(&self, kind: ServiceKind) -> PathBuf {
        let name = match self.platform {
            Platform::Launchd => format!("{}.plist", kind.launchd_label()),
            Platform::Systemd => kind.systemd_unit().to_string(),
        };
        self.unit_dir.join(name)
    }

    pub(crate) fn render(&self, spec: &Spec) -> String {
        match self.platform {
            Platform::Launchd => launchd::render(spec),
            Platform::Systemd => systemd::render(spec),
        }
    }

    fn domain(&self) -> String {
        format!("gui/{}", self.uid)
    }

    fn target(&self, kind: ServiceKind) -> String {
        format!("{}/{}", self.domain(), kind.launchd_label())
    }

    fn systemctl(args: &[&str]) -> Call {
        let mut all = vec!["--user"];
        all.extend_from_slice(args);
        Call::new("systemctl", &all)
    }

    pub(crate) fn state_call(&self, kind: ServiceKind) -> Call {
        match self.platform {
            Platform::Launchd => Call::new("launchctl", &["print", &self.target(kind)]),
            Platform::Systemd => Self::systemctl(&[
                "show",
                kind.systemd_unit(),
                "--property=LoadState,ActiveState,SubState,MainPID",
            ]),
        }
    }

    pub(crate) fn parse_state(&self, captured: &Captured) -> Activity {
        match self.platform {
            Platform::Launchd => state::from_launchctl_print(captured),
            Platform::Systemd => state::from_systemctl_show(captured),
        }
    }

    fn bootstrap(&self, kind: ServiceKind) -> Call {
        let file = self.file_path(kind).display().to_string();
        Call::new("launchctl", &["bootstrap", &self.domain(), &file])
    }

    fn bootout(&self, kind: ServiceKind) -> Call {
        Call::new("launchctl", &["bootout", &self.target(kind)])
    }

    pub(crate) fn install_calls(
        &self,
        kind: ServiceKind,
        before: &Activity,
        file_changed: bool,
    ) -> Vec<Call> {
        match self.platform {
            Platform::Launchd => self.launchd_install_calls(kind, before, file_changed),
            Platform::Systemd => {
                let unit = kind.systemd_unit();
                let mut calls = Vec::new();
                if file_changed {
                    calls.push(Self::systemctl(&["daemon-reload"]));
                }
                calls.push(Self::systemctl(&["enable", "--now", unit]));
                if file_changed && before.is_running() {
                    calls.push(Self::systemctl(&["restart", unit]));
                }
                calls
            }
        }
    }

    fn launchd_install_calls(
        &self,
        kind: ServiceKind,
        before: &Activity,
        file_changed: bool,
    ) -> Vec<Call> {
        match (before.is_loaded(), file_changed, before.is_running()) {
            (false, _, _) => vec![self.bootstrap(kind)],
            (true, true, _) => vec![self.bootout(kind), self.bootstrap(kind)],
            (true, false, true) => Vec::new(),
            (true, false, false) => vec![self.kickstart(kind, false)],
        }
    }

    fn kickstart(&self, kind: ServiceKind, kill_first: bool) -> Call {
        let target = self.target(kind);
        if kill_first {
            Call::new("launchctl", &["kickstart", "-k", &target])
        } else {
            Call::new("launchctl", &["kickstart", &target])
        }
    }

    pub(crate) fn restart_calls(&self, kind: ServiceKind, before: &Activity) -> Vec<Call> {
        match self.platform {
            Platform::Launchd if before.is_loaded() => vec![self.kickstart(kind, true)],
            Platform::Launchd => vec![self.bootstrap(kind)],
            Platform::Systemd => vec![Self::systemctl(&["restart", kind.systemd_unit()])],
        }
    }

    pub(crate) fn start_calls(&self, kind: ServiceKind, before: &Activity) -> Vec<Call> {
        match self.platform {
            Platform::Launchd if before.is_loaded() => vec![self.kickstart(kind, false)],
            Platform::Launchd => vec![self.bootstrap(kind)],
            Platform::Systemd => {
                vec![Self::systemctl(&[
                    "start",
                    "--no-block",
                    kind.systemd_unit(),
                ])]
            }
        }
    }

    pub(crate) fn uninstall_calls(&self, kind: ServiceKind) -> Vec<Call> {
        match self.platform {
            Platform::Launchd => vec![self.bootout(kind)],
            Platform::Systemd => {
                vec![Self::systemctl(&["disable", "--now", kind.systemd_unit()])]
            }
        }
    }

    pub(crate) fn after_removal_calls(&self) -> Vec<Call> {
        match self.platform {
            Platform::Launchd => Vec::new(),
            Platform::Systemd => vec![Self::systemctl(&["daemon-reload"])],
        }
    }
}
