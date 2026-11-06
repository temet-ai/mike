mod credentials;
mod down_status;
mod entry;
mod native;
mod up;

use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::sync::Mutex;

use super::runner::{Call, Captured, Runner};
use super::server::{Server, ServerError};
use super::up::{Paths, Timing};

pub(super) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(super) fn ok(stdout: &str) -> io::Result<Captured> {
    Ok(Captured {
        success: true,
        stdout: stdout.to_string(),
        stderr: String::new(),
    })
}

pub(super) fn failed(stderr: &str) -> io::Result<Captured> {
    Ok(Captured {
        success: false,
        stdout: String::new(),
        stderr: stderr.to_string(),
    })
}

pub(super) fn not_found() -> io::Result<Captured> {
    Err(io::Error::from(io::ErrorKind::NotFound))
}

#[derive(Clone, Copy)]
pub(super) enum Daemon {
    Up,
    Down,
    Missing,
}

#[derive(Clone, Copy)]
pub(super) enum ComposeCli {
    Plugin,
    Standalone,
    Neither,
}

pub(super) struct Docker {
    pub(super) daemon: Daemon,
    pub(super) compose: ComposeCli,
    pub(super) volume_present: bool,
    pub(super) up_failure: Option<&'static str>,
    inspections: Mutex<VecDeque<&'static str>>,
    calls: Mutex<Vec<Call>>,
}

impl Docker {
    pub(super) fn new(inspections: &[&'static str]) -> Self {
        Self {
            daemon: Daemon::Up,
            compose: ComposeCli::Plugin,
            volume_present: true,
            up_failure: None,
            inspections: Mutex::new(inspections.iter().copied().collect()),
            calls: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn calls(&self) -> Vec<Call> {
        lock(&self.calls).clone()
    }

    pub(super) fn shown(&self) -> Vec<String> {
        self.calls().iter().map(Call::shown).collect()
    }

    pub(super) fn call_with(&self, verb: &str) -> Option<Call> {
        self.calls()
            .into_iter()
            .find(|c| c.args.iter().any(|a| a == verb))
    }

    fn next_inspection(&self) -> &'static str {
        let mut queue = lock(&self.inspections);
        if queue.len() > 1 {
            queue.pop_front().unwrap_or("absent")
        } else {
            queue.front().copied().unwrap_or("absent")
        }
    }

    fn respond(&self, call: &Call) -> io::Result<Captured> {
        let has = |word: &str| call.args.iter().any(|a| a == word);
        match (call.program.as_str(), call.args.first().map(String::as_str)) {
            ("docker", Some("info")) => match self.daemon {
                Daemon::Up => ok("server info"),
                Daemon::Down => failed("Cannot connect to the Docker daemon\nsecond line"),
                Daemon::Missing => not_found(),
            },
            ("docker", Some("compose")) if has("version") => match self.compose {
                ComposeCli::Plugin => ok("Docker Compose version v2"),
                _ => failed("docker: 'compose' is not a docker command"),
            },
            ("docker-compose", Some("version")) => match self.compose {
                ComposeCli::Standalone => ok("docker-compose 1.29"),
                _ => not_found(),
            },
            ("docker", Some("container")) => match self.next_inspection() {
                "absent" => failed("Error: No such container: mike-db"),
                state => ok(&format!("{state}\n")),
            },
            ("docker", Some("volume")) if self.volume_present => ok("[]"),
            ("docker", Some("volume")) => failed("Error: No such volume"),
            ("docker", Some("logs")) => ok("database system is ready"),
            (_, _) if has("up") => self.up_failure.map_or_else(|| ok(""), failed),
            (_, _) if has("down") => ok(""),
            _ => Err(io::Error::other(format!(
                "unscripted call {}",
                call.shown()
            ))),
        }
    }
}

impl Runner for Docker {
    fn run(&self, call: &Call) -> impl Future<Output = io::Result<Captured>> + Send {
        lock(&self.calls).push(call.clone());
        std::future::ready(self.respond(call))
    }
}

pub(super) enum Reply {
    Applied(usize),
    Auth,
    Other(&'static str),
}

pub(super) struct FakeServer {
    pub(super) reply: Reply,
    pub(super) urls: Mutex<Vec<String>>,
}

impl FakeServer {
    pub(super) fn new(reply: Reply) -> Self {
        Self {
            reply,
            urls: Mutex::new(Vec::new()),
        }
    }
}

impl Server for FakeServer {
    fn migrate(&self, url: &str) -> impl Future<Output = Result<usize, ServerError>> {
        lock(&self.urls).push(url.to_string());
        std::future::ready(match &self.reply {
            Reply::Applied(n) => Ok(*n),
            Reply::Auth => Err(ServerError::Auth(
                "password authentication failed".to_string(),
            )),
            Reply::Other(why) => Err(ServerError::Other((*why).to_string())),
        })
    }
}

pub(super) fn quick() -> Timing {
    Timing {
        daemon_wait: std::time::Duration::from_secs(5),
        health_polls: 3,
        poll: std::time::Duration::ZERO,
    }
}

pub(super) struct Home {
    pub(super) _dir: tempfile::TempDir,
    pub(super) paths: Paths,
}

pub(super) fn home() -> anyhow::Result<Home> {
    let dir = tempfile::tempdir()?;
    let paths = Paths {
        config_file: dir.path().join("config").join("mike").join("config.toml"),
        state_dir: dir.path().join("state").join("mike"),
    };
    Ok(Home { _dir: dir, paths })
}

pub(super) fn read_config(paths: &Paths) -> String {
    std::fs::read_to_string(&paths.config_file).unwrap_or_default()
}
