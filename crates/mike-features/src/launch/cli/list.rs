use serde::Serialize;
use uuid::Uuid;

use super::runner::Runner;
use crate::launch::{LaunchError, Surface};
use crate::store::persistence::launch::{LaunchListFilter, LaunchRow};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListRow {
    pub surface: String,
    pub folder: String,
    pub session: Option<Uuid>,
    pub agent_state: Option<String>,
    pub run: Option<String>,
    pub seat: Option<String>,
}

impl ListRow {
    pub fn to_line(&self) -> String {
        let dash = |s: Option<&str>| s.unwrap_or("-").to_string();
        format!(
            "{}\t{}\t{}\t{}\t{}/{}",
            self.surface,
            self.folder,
            self.session.map_or("-".to_string(), |u| u.to_string()),
            dash(self.agent_state.as_deref()),
            dash(self.run.as_deref()),
            dash(self.seat.as_deref()),
        )
    }
}

pub fn merge(rows: Vec<LaunchRow>, surfaces: Vec<Surface>, run_scoped: bool) -> Vec<ListRow> {
    let mut remaining = surfaces;
    let mut out = Vec::with_capacity(remaining.len());
    for row in rows {
        let Some(session) = row.session_uuid else {
            continue;
        };
        let Some(pos) = remaining
            .iter()
            .position(|s| s.session_uuid == Some(session))
        else {
            continue;
        };
        let surface = remaining.remove(pos);
        out.push(ListRow {
            surface: surface.surface_ref,
            folder: row.folder,
            session: Some(session),
            agent_state: surface.agent_state,
            run: row.run_id,
            seat: row.seat,
        });
    }
    if run_scoped {
        return out;
    }
    remaining.sort_by(|a, b| a.surface_ref.cmp(&b.surface_ref));
    out.extend(remaining.into_iter().map(|surface| ListRow {
        surface: surface.surface_ref,
        folder: surface.cwd.unwrap_or_default(),
        session: surface.session_uuid,
        agent_state: surface.agent_state,
        run: None,
        seat: None,
    }));
    out
}

pub async fn execute(runner: &Runner<'_>, run: Option<&str>) -> Result<Vec<ListRow>, LaunchError> {
    let surfaces = runner.backend.list().await?;
    let filter = LaunchListFilter {
        run_id: run.map(str::to_string),
        open_only: true,
        backend: Some(runner.backend.id().as_str().to_string()),
        limit: LaunchListFilter::DEFAULT_LIMIT,
    };
    let rows = runner
        .ledger
        .list_launches(&filter)
        .await
        .unwrap_or_default();
    Ok(merge(rows, surfaces, run.is_some()))
}
