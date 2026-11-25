use std::path::{Path, PathBuf};

use crate::store::persistence::PersistHandle;
use crate::store::persistence::wright::Fixer;

use super::runner::{Cmd, CommandRunner, Program, checked};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Here {
    pub(crate) host: String,
    pub(crate) checkout: Option<PathBuf>,
    pub(crate) loadavg: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Place {
    Local,
    Remote(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) machine: String,
    pub(crate) place: Place,
    pub(crate) folder: String,
}

pub(crate) struct FollowupContext<R, S> {
    pub(crate) persist: PersistHandle,
    pub(crate) runner: R,
    pub(crate) sessions: S,
    pub(crate) here: Here,
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

pub(crate) fn is_checkout(dir: &Path) -> bool {
    dir.join("crates/mike/Cargo.toml").is_file()
}

fn resolve_checkout() -> Option<PathBuf> {
    env_path("MIKE_WRIGHT_REPO")
        .or_else(|| std::env::current_dir().ok())
        .filter(|dir| is_checkout(dir))
}

fn usable(raw: Option<&str>) -> Option<&str> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty() && !value.starts_with('-'))
}

impl Here {
    pub(crate) async fn detect() -> Self {
        Self {
            host: crate::wright::investigate::context::machine().await.host,
            checkout: resolve_checkout(),
            loadavg: PathBuf::from("/proc/loadavg"),
        }
    }

    fn local(&self, machine: &str, folder: Option<&str>) -> Option<Target> {
        let folder = match usable(folder) {
            Some(folder) => folder.to_owned(),
            None => self.checkout.as_ref()?.display().to_string(),
        };
        Some(Target {
            machine: machine.to_owned(),
            place: Place::Local,
            folder,
        })
    }

    fn target(&self, fixer: &Fixer) -> Option<Target> {
        match usable(fixer.host.as_deref()) {
            Some(host) => Some(Target {
                machine: fixer.machine.clone(),
                place: Place::Remote(host.to_owned()),
                folder: usable(fixer.folder.as_deref())?.to_owned(),
            }),
            None if fixer.machine.eq_ignore_ascii_case(&self.host) => {
                self.local(&fixer.machine, fixer.folder.as_deref())
            }
            None => None,
        }
    }

    pub(crate) fn targets(&self, fixers: &[Fixer]) -> Vec<Target> {
        if fixers.is_empty() {
            return self.local(&self.host, None).into_iter().collect();
        }
        fixers
            .iter()
            .filter_map(|fixer| self.target(fixer))
            .collect()
    }

    pub(crate) fn fix_targets(&self, fixers: &[Fixer], fix_host: &str) -> Vec<Target> {
        let mut targets = self.targets(fixers);
        targets.retain(|target| target.machine.eq_ignore_ascii_case(fix_host.trim()));
        targets
    }

    pub(crate) fn unreachable(&self, fixers: &[Fixer], fix_host: &str) -> Option<String> {
        if !self.fix_targets(fixers, fix_host).is_empty() {
            return None;
        }
        if !self.targets(fixers).is_empty() {
            return Some(format!(
                "{} cannot start a fixer on the fix host {fix_host}; the follow-up work is left to a machine that can",
                self.host
            ));
        }
        Some(if fixers.is_empty() {
            format!(
                "{} has no mike checkout and no fixer machines are set; run mike serve from the checkout or set MIKE_WRIGHT_REPO",
                self.host
            )
        } else {
            format!(
                "none of the {} fixer machines can be reached from {}: each needs a host and folder, or this machine's name",
                fixers.len(),
                self.host
            )
        })
    }
}

pub(crate) async fn gh_ready<R: CommandRunner>(runner: &R) -> Result<(), String> {
    checked(runner, &Cmd::new(Program::Gh, &["auth", "status"]))
        .await
        .map(|_| ())
        .map_err(|e| format!("gh cannot file issues here: {e}"))
}
