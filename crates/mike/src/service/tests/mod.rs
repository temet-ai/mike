mod commands;
mod files;
mod health;
mod install;
mod lifecycle;
mod paths;
mod render;

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::endpoints::Endpoints;
use super::environment::{self, Inputs};
use super::manager::Manager;
use super::platform::Platform;
use super::spec::Spec;
use crate::db_provision::{Call, Captured, Runner};

pub(super) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(super) fn inputs(home: &Path) -> Inputs {
    Inputs {
        home: home.to_path_buf(),
        exe: PathBuf::from("/opt/homebrew/bin/mike"),
        xdg_config_home: None,
        xdg_state_home: None,
        mise_data_dir: None,
        path_var: None,
    }
}

pub(super) fn specs_in(home: &Path) -> Vec<Spec> {
    environment::specs(&inputs(home), &home.join(".local/state/mike"))
}

pub(super) fn manager(platform: Platform, home: &Path) -> Manager {
    Manager::new(platform, home, None, 501)
}

pub(super) fn endpoints() -> Endpoints {
    Endpoints {
        proxy_addr: "127.0.0.1:9988".to_string(),
        inspect_addr: "127.0.0.1:9989".to_string(),
        spa_addr: Some("127.0.0.1:9990".to_string()),
        sidecar_url: "http://127.0.0.1:9987".to_string(),
    }
}

pub(super) fn nothing_listens(_: &str) -> bool {
    false
}

pub(super) fn everything_listens(_: &str) -> bool {
    true
}

pub(super) fn inspect_port_listens(addr: &str) -> bool {
    addr.ends_with(":9989")
}

pub(super) struct Fake {
    pub(super) calls: Mutex<Vec<String>>,
    loaded: Mutex<BTreeSet<String>>,
    dev_units: String,
    failing: Option<&'static str>,
}

fn answer(success: bool, stdout: &str, stderr: &str) -> io::Result<Captured> {
    Ok(Captured {
        success,
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
    })
}

impl Fake {
    pub(super) fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            loaded: Mutex::new(BTreeSet::new()),
            dev_units: String::new(),
            failing: None,
        }
    }

    pub(super) fn with_loaded(self, identifiers: &[&str]) -> Self {
        lock(&self.loaded).extend(identifiers.iter().map(ToString::to_string));
        self
    }

    pub(super) fn with_dev_units(mut self, listing: &str) -> Self {
        self.dev_units = listing.to_string();
        self
    }

    pub(super) fn failing_on(mut self, fragment: &'static str) -> Self {
        self.failing = Some(fragment);
        self
    }

    pub(super) fn seen(&self) -> Vec<String> {
        lock(&self.calls).clone()
    }

    pub(super) fn count_containing(&self, fragment: &str) -> usize {
        self.seen().iter().filter(|c| c.contains(fragment)).count()
    }

    pub(super) fn position_of(&self, fragment: &str) -> Option<usize> {
        self.seen().iter().position(|c| c.contains(fragment))
    }

    fn is_loaded(&self, identifier: &str) -> bool {
        lock(&self.loaded).contains(identifier)
    }

    fn set_loaded(&self, identifier: &str, loaded: bool) {
        let mut set = lock(&self.loaded);
        if loaded {
            set.insert(identifier.to_string());
        } else {
            set.remove(identifier);
        }
    }

    fn launchctl(&self, args: &[String]) -> io::Result<Captured> {
        let last = args.last().map_or("", String::as_str);
        let label = last.rsplit('/').next().unwrap_or(last);
        match args.first().map(String::as_str) {
            Some("print") if self.is_loaded(label) => {
                answer(true, "\tstate = running\n\tpid = 77\n", "")
            }
            Some("print") => answer(false, "", "Could not find service"),
            Some("bootstrap") => {
                self.set_loaded(label.trim_end_matches(".plist"), true);
                answer(true, "", "")
            }
            Some("bootout") => {
                self.set_loaded(label, false);
                answer(true, "", "")
            }
            _ => answer(true, "", ""),
        }
    }

    fn systemctl(&self, args: &[String]) -> io::Result<Captured> {
        let unit = args.last().map_or("", String::as_str);
        let verb = args.get(1).map_or("", String::as_str);
        match verb {
            "list-units" => answer(true, &self.dev_units, ""),
            "show" => {
                let unit = args.get(2).map_or("", String::as_str);
                if self.is_loaded(unit) {
                    answer(
                        true,
                        "LoadState=loaded\nActiveState=active\nSubState=running\nMainPID=77\n",
                        "",
                    )
                } else {
                    answer(
                        true,
                        "LoadState=not-found\nActiveState=inactive\nSubState=dead\nMainPID=0\n",
                        "",
                    )
                }
            }
            "enable" => {
                self.set_loaded(unit, true);
                answer(true, "", "")
            }
            "disable" => {
                self.set_loaded(unit, false);
                answer(true, "", "")
            }
            _ => answer(true, "", ""),
        }
    }
}

impl Runner for Fake {
    async fn run(&self, call: &Call) -> io::Result<Captured> {
        let shown = call.shown();
        lock(&self.calls).push(shown.clone());
        if self
            .failing
            .is_some_and(|fragment| shown.contains(fragment))
        {
            return answer(false, "", "boom");
        }
        match call.program.as_str() {
            "launchctl" => self.launchctl(&call.args),
            "systemctl" => self.systemctl(&call.args),
            other => Err(io::Error::other(format!("unexpected program {other}"))),
        }
    }
}
