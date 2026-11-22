use std::path::{Path, PathBuf};

use serde::Serialize;

use super::display::progress_line;
use super::status::FleetStatus;
use crate::store::persistence::fleet::FleetRow;

pub const JSON_NAME: &str = "fleets-live.json";
pub const TEXT_NAME: &str = "fleets-live.txt";

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct LiveFleet {
    pub slug: String,
    pub fleet_uuid: String,
    pub project_path: String,
    pub summary: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Live {
    pub fleets: Vec<LiveFleet>,
}

impl Live {
    pub fn of(running: &[(&FleetRow, &FleetStatus)]) -> Self {
        Self {
            fleets: running
                .iter()
                .map(|(row, status)| LiveFleet {
                    slug: row.slug.clone(),
                    fleet_uuid: row.fleet_uuid.to_string(),
                    project_path: row.project_path.clone(),
                    summary: format!("{} {}", row.slug, progress_line(status)),
                })
                .collect(),
        }
    }

    pub fn text(&self) -> String {
        self.fleets
            .first()
            .map(|f| format!("{}\n", f.summary))
            .unwrap_or_default()
    }
}

pub fn dir() -> Option<PathBuf> {
    mike_base::machine_identity::default_state_dir().ok()
}

pub async fn write_if_changed(dir: &Path, live: &Live, last: &mut Option<String>) {
    let Ok(json) = serde_json::to_string(live) else {
        return;
    };
    if last.as_deref() == Some(json.as_str()) {
        return;
    }
    match write_both(dir, &json, &live.text()).await {
        Ok(()) => *last = Some(json),
        Err(error) => {
            tracing::warn!(%error, dir = %dir.display(), "fleet: the live-fleet file could not be written");
        }
    }
}

async fn write_both(dir: &Path, json: &str, text: &str) -> std::io::Result<()> {
    tokio::fs::create_dir_all(dir).await?;
    replace(&dir.join(JSON_NAME), json).await?;
    replace(&dir.join(TEXT_NAME), text).await
}

async fn replace(path: &Path, contents: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    tokio::fs::write(&tmp, contents).await?;
    tokio::fs::rename(&tmp, path).await
}

#[cfg(test)]
#[path = "live_file_tests.rs"]
mod tests;
