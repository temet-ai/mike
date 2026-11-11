use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::ledger_wire::ClosedBody;
use crate::store::persistence::launch::{LaunchOutcome, LaunchSurfaceRefs, NewLaunch, NewRun};

const DIR: &str = "launch-queue";
const RUN: &str = "run-";
const LAUNCH: &str = "launch-";
const CLOSE: &str = "close-";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct QueuedLaunch {
    pub(crate) launch: NewLaunch,
    pub(crate) opened: Option<LaunchSurfaceRefs>,
    pub(crate) outcome: Option<LaunchOutcome>,
    pub(crate) closed: Option<ClosedBody>,
    #[serde(default)]
    pub(crate) attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct QueuedClose {
    pub(crate) session: Uuid,
    pub(crate) closed: ClosedBody,
    #[serde(default)]
    pub(crate) attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalQueue {
    dir: PathBuf,
}

fn safe(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn local_id(key: u32) -> i64 {
    -i64::from(key.max(1))
}

fn read<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

impl LocalQueue {
    pub(crate) fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub(crate) fn default_dir() -> Option<Self> {
        crate::machine_identity::default_state_dir()
            .ok()
            .map(|state| Self::at(state.join(DIR)))
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    fn write<T: Serialize>(&self, name: &str, value: &T) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| format!("could not create {}: {e}", self.dir.display()))?;
        let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
        let path = self.dir.join(format!("{name}.json"));
        let staged = self.dir.join(format!(".{name}.json.tmp"));
        std::fs::write(&staged, text)
            .and_then(|()| std::fs::rename(&staged, &path))
            .map_err(|e| format!("could not write {}: {e}", path.display()))
    }

    fn named<T: DeserializeOwned>(&self, prefix: &str) -> Vec<(PathBuf, T)> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut found: Vec<(PathBuf, T)> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(prefix) && n.ends_with(".json"))
            })
            .filter_map(|p| read(&p).ok().map(|v| (p, v)))
            .collect();
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found
    }

    pub(crate) fn save_run(&self, run: &NewRun) -> Result<(), String> {
        self.write(&format!("{RUN}{}", safe(&run.run_id)), run)
    }

    pub(crate) fn save_launch(&self, launch: &NewLaunch) -> Result<i64, String> {
        let key = u32::try_from(Uuid::new_v4().as_u128() & 0x7fff_ffff).unwrap_or(1);
        let mut launch = launch.clone();
        if launch.session_uuid.is_some()
            && launch.brief_text.is_some()
            && launch.brief_note_uuid.is_none()
        {
            launch.brief_note_uuid = Some(Uuid::new_v4());
        }
        let queued = QueuedLaunch {
            launch,
            opened: None,
            outcome: None,
            closed: None,
            attempts: 0,
        };
        self.write(&format!("{LAUNCH}{}", key.max(1)), &queued)?;
        Ok(local_id(key))
    }

    fn launch_name(id: i64) -> Option<String> {
        (id < 0).then(|| format!("{LAUNCH}{}", id.unsigned_abs()))
    }

    pub(crate) fn update_launch(
        &self,
        id: i64,
        change: impl FnOnce(&mut QueuedLaunch),
    ) -> Result<(), String> {
        let name = Self::launch_name(id).ok_or_else(|| format!("launch {id} is not queued"))?;
        let mut queued: QueuedLaunch = read(&self.dir.join(format!("{name}.json")))?;
        change(&mut queued);
        self.write(&name, &queued)
    }

    pub(crate) fn save_close(&self, session: Uuid, closed: ClosedBody) -> Result<(), String> {
        let queued_launch = self
            .launches()
            .into_iter()
            .find(|(_, q)| q.launch.session_uuid == Some(session));
        if let Some((path, mut queued)) = queued_launch {
            queued.closed = Some(closed);
            let name = path
                .file_stem()
                .and_then(|n| n.to_str())
                .ok_or_else(|| format!("{} has no name", path.display()))?
                .to_string();
            return self.write(&name, &queued);
        }
        let queued = QueuedClose {
            session,
            closed,
            attempts: 0,
        };
        self.write(&format!("{CLOSE}{session}"), &queued)
    }

    pub(crate) fn save_named<T: Serialize>(&self, path: &Path, value: &T) -> Result<(), String> {
        let name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("{} has no name", path.display()))?;
        self.write(name, value)
    }

    pub(crate) fn runs(&self) -> Vec<(PathBuf, NewRun)> {
        self.named(RUN)
    }

    pub(crate) fn launches(&self) -> Vec<(PathBuf, QueuedLaunch)> {
        self.named(LAUNCH)
    }

    pub(crate) fn closes(&self) -> Vec<(PathBuf, QueuedClose)> {
        self.named(CLOSE)
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.runs().is_empty() && self.launches().is_empty() && self.closes().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_queued_launch_takes_updates_and_a_close_by_its_session() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let queue = LocalQueue::at(dir.path().join("q"));
        let session = Uuid::new_v4();
        let mut launch = NewLaunch::new("herdr", "/proj");
        launch.session_uuid = Some(session);
        let id = queue.save_launch(&launch).map_err(anyhow::Error::msg)?;
        assert!(id < 0);
        queue
            .update_launch(id, |q| q.opened = Some(LaunchSurfaceRefs::default()))
            .map_err(anyhow::Error::msg)?;
        let closed = ClosedBody {
            reason: "kill --session".into(),
            process_exited: Some(true),
        };
        queue
            .save_close(session, closed.clone())
            .map_err(anyhow::Error::msg)?;
        let launches = queue.launches();
        assert_eq!(launches.len(), 1);
        assert!(launches[0].1.opened.is_some());
        assert_eq!(launches[0].1.closed, Some(closed));
        assert!(queue.closes().is_empty());
        Ok(())
    }

    #[test]
    fn a_close_for_an_unqueued_session_and_a_run_are_kept_apart() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let queue = LocalQueue::at(dir.path().to_path_buf());
        assert!(queue.is_empty());
        let run = NewRun {
            run_id: "fleet/x 1".into(),
            project_path: None,
            backend: "herdr".into(),
            workspace_ref: None,
            tab_ref: None,
        };
        queue.save_run(&run).map_err(anyhow::Error::msg)?;
        let closed = ClosedBody {
            reason: "kill --session".into(),
            process_exited: None,
        };
        queue
            .save_close(Uuid::new_v4(), closed)
            .map_err(anyhow::Error::msg)?;
        assert_eq!(queue.runs()[0].1, run);
        assert_eq!(queue.closes().len(), 1);
        assert!(queue.update_launch(5, |_| {}).is_err());
        Ok(())
    }

    #[test]
    fn an_unwritable_queue_says_where_it_failed() {
        let queue = LocalQueue::at(PathBuf::from("/proc/nonexistent/queue"));
        let err = queue.save_launch(&NewLaunch::new("herdr", "/proj")).err();
        assert!(err.is_some_and(|e| e.contains("/proc/nonexistent/queue")));
    }
}
