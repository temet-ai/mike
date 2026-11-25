use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use regex::Regex;

use super::working::worked_within;
use crate::change_record::{ShellCommand, WorkingCheckout};

const READ_EVERY: Duration = Duration::from_secs(1);

const LATE_ROWS: i64 = 200;

#[derive(Default)]
pub(super) struct ShellFolders {
    folders: HashMap<String, WorkingCheckout>,
    entered: HashMap<String, DateTime<Utc>>,
    newest_id: i64,
    read_at: Option<Instant>,
}

impl ShellFolders {
    pub(super) fn wait(&self) -> Duration {
        self.read_at
            .map_or(Duration::ZERO, |at| READ_EVERY.saturating_sub(at.elapsed()))
    }

    pub(super) fn since(&mut self, now: DateTime<Utc>) -> (DateTime<Utc>, i64) {
        self.read_at = Some(Instant::now());
        (now - worked_within(), (self.newest_id - LATE_ROWS).max(0))
    }

    pub(super) fn add(&mut self, commands: &[ShellCommand], now: DateTime<Utc>) {
        let window = now - worked_within();
        for command in commands {
            self.newest_id = self.newest_id.max(command.id);
            for folder in folders_named(&command.command, &command.cwd) {
                self.touch(folder, command);
            }
        }
        self.folders
            .retain(|_, folder| folder.newest_request_at >= window);
        let held = &self.folders;
        self.entered.retain(|folder, _| held.contains_key(folder));
    }

    fn touch(&mut self, folder: String, command: &ShellCommand) {
        let entered = self.entered.entry(folder.clone()).or_insert(command.at);
        *entered = (*entered).min(command.at);
        let held = self
            .folders
            .entry(folder)
            .or_insert_with_key(|folder| WorkingCheckout {
                checkout: folder.clone(),
                project_id: command.project_id,
                newest_request_at: command.at,
                sessions: vec![command.session],
            });
        held.newest_request_at = held.newest_request_at.max(command.at);
    }

    pub(super) fn entered(&self, folder: &str) -> Option<DateTime<Utc>> {
        self.entered.get(folder).copied()
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
