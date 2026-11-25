use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use regex::Regex;

use super::working::worked_within;
use crate::change_record::{ShellCommand, WorkingCheckout};

const READ_EVERY: Duration = Duration::from_secs(1);

fn overlap() -> chrono::Duration {
    chrono::Duration::seconds(30)
}

#[derive(Default)]
pub(super) struct ShellFolders {
    folders: HashMap<String, WorkingCheckout>,
    newest: Option<DateTime<Utc>>,
    read_at: Option<Instant>,
}

impl ShellFolders {
    pub(super) fn wait(&self) -> Duration {
        self.read_at
            .map_or(Duration::ZERO, |at| READ_EVERY.saturating_sub(at.elapsed()))
    }

    pub(super) fn since(&mut self, now: DateTime<Utc>) -> DateTime<Utc> {
        self.read_at = Some(Instant::now());
        let window = now - worked_within();
        self.newest
            .map_or(window, |at| (at - overlap()).max(window))
    }

    pub(super) fn add(&mut self, commands: &[ShellCommand], now: DateTime<Utc>) -> bool {
        let window = now - worked_within();
        self.folders
            .retain(|_, folder| folder.newest_request_at >= window);
        let mut joined = false;
        for command in commands {
            self.newest = Some(self.newest.map_or(command.at, |at| at.max(command.at)));
            for folder in folders_named(&command.command, &command.cwd) {
                joined |= self.touch(folder, command);
            }
        }
        joined
    }

    fn touch(&mut self, folder: String, command: &ShellCommand) -> bool {
        if let Some(held) = self.folders.get_mut(&folder) {
            held.newest_request_at = held.newest_request_at.max(command.at);
            return false;
        }
        let checkout = WorkingCheckout {
            checkout: folder.clone(),
            project_id: command.project_id,
            newest_request_at: command.at,
            sessions: vec![command.session],
        };
        self.folders.insert(folder, checkout);
        true
    }

    pub(super) fn rows(&self) -> Vec<WorkingCheckout> {
        self.folders.values().cloned().collect()
    }

    pub(super) fn forget(&mut self) {
        *self = Self::default();
    }
}

fn pattern() -> Option<&'static Regex> {
    static PATTERN: OnceLock<Option<Regex>> = OnceLock::new();
    PATTERN
        .get_or_init(|| {
            Regex::new(
                r#"(?:^|[\s;&|(])(?:(?:cd|pushd|git\s+-C)\s+|[A-Za-z_]\w*=)(?:"([^"\n]+)"|'([^'\n]+)'|([^\s;&|()"'<>]+))"#,
            )
            .ok()
        })
        .as_ref()
}

pub fn folders_named(command: &str, cwd: &str) -> Vec<String> {
    let Some(pattern) = pattern() else {
        return Vec::new();
    };
    let mut named = Vec::new();
    for found in pattern.captures_iter(command) {
        let Some(text) = found.get(1).or(found.get(2)).or(found.get(3)) else {
            continue;
        };
        if let Some(folder) = absolute(text.as_str(), cwd)
            && folder != cwd
            && !named.contains(&folder)
        {
            named.push(folder);
        }
    }
    named
}

fn absolute(text: &str, cwd: &str) -> Option<String> {
    if text.contains(['$', '~', '*', '`', '{']) || !text.contains('/') || !cwd.starts_with('/') {
        return None;
    }
    let mut out = PathBuf::new();
    for part in Path::new(cwd).join(text).components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    (out.components().count() > 1).then(|| out.to_string_lossy().into_owned())
}
