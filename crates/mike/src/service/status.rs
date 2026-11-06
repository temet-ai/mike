use std::path::PathBuf;

use super::files::{self, Presence};
use super::manager::Manager;
use super::spec::ServiceKind;
use super::state::Activity;
use crate::db_provision::Runner;

#[derive(Debug, Clone)]
pub(crate) struct UnitFacts {
    pub(crate) kind: ServiceKind,
    pub(crate) identifier: &'static str,
    pub(crate) file: PathBuf,
    pub(crate) presence: Presence,
    pub(crate) activity: Activity,
}

impl UnitFacts {
    pub(crate) fn is_up(&self) -> bool {
        self.presence == Presence::Ours && self.activity.is_running()
    }

    pub(crate) fn describe(&self) -> String {
        match self.presence {
            Presence::Missing => format!("{}: not installed", self.kind.name()),
            Presence::Foreign => format!(
                "{}: {} exists but mike did not write it",
                self.kind.name(),
                self.file.display()
            ),
            Presence::Ours => format!(
                "{}: installed as {} ({}), {}",
                self.kind.name(),
                self.identifier,
                self.file.display(),
                self.activity.describe()
            ),
        }
    }
}

pub(crate) async fn collect<R: Runner>(runner: &R, manager: &Manager) -> Vec<UnitFacts> {
    let mut units = Vec::new();
    for kind in ServiceKind::ALL {
        let file = manager.file_path(kind);
        let (presence, activity) = match files::inspect(&file) {
            Ok(existing) if existing.presence() == Presence::Ours => (
                Presence::Ours,
                super::exec::activity(runner, manager, kind)
                    .await
                    .unwrap_or_else(|e| Activity::Unknown(e.to_string())),
            ),
            Ok(existing) => (existing.presence(), Activity::NotLoaded),
            Err(e) => (Presence::Foreign, Activity::Unknown(e.to_string())),
        };
        units.push(UnitFacts {
            kind,
            identifier: manager.identifier(kind),
            file,
            presence,
            activity,
        });
    }
    units
}
