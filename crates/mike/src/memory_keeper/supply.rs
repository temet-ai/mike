use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::arm::{KeeperArm, arm_for};
use super::block::{Block, render};
use super::resolve::{Resolution, ResolveInput, SessionFacts, resolve};
use super::snapshot::KeeperSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Carrier {
    Tether,
    Mod,
    Proxy,
}

impl Carrier {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Tether => "tether",
            Self::Mod => "mod",
            Self::Proxy => "proxy",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SupplyRequest<'a> {
    pub(crate) session: Uuid,
    pub(crate) project_path: &'a str,
    pub(crate) prompt: Option<&'a str>,
    pub(crate) cwd: Option<&'a str>,
    pub(crate) branch: Option<&'a str>,
    pub(crate) recent: Vec<&'a str>,
    pub(crate) carrier: Carrier,
    pub(crate) now: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SupplyRecord {
    pub(crate) session: Uuid,
    pub(crate) thread_slug: Option<String>,
    pub(crate) content_hash: Option<String>,
    pub(crate) carrier: Carrier,
    pub(crate) arm: KeeperArm,
    pub(crate) resolution: &'static str,
    pub(crate) signals: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Supply {
    pub(crate) block: Option<Block>,
    pub(crate) record: Option<SupplyRecord>,
}

impl Supply {
    fn nothing() -> Self {
        Self {
            block: None,
            record: None,
        }
    }
}

pub(crate) fn supply(snapshot: &KeeperSnapshot, request: &SupplyRequest<'_>) -> Supply {
    let Some(project) = snapshot.project_for(request.project_path) else {
        return Supply::nothing();
    };
    let threads = project.facts();
    if threads.is_empty() {
        return Supply::nothing();
    }
    let resolution = resolve(&ResolveInput {
        prompt: request.prompt,
        session: SessionFacts {
            session_uuid: request.session,
            cwd: request.cwd,
            branch: request.branch,
            recent_turn_text: request.recent.clone(),
        },
        threads: &threads,
        now: request.now,
    });
    let arm = arm_for(&request.session.to_string());
    let block = (arm == KeeperArm::Keeper)
        .then(|| render(project, &resolution))
        .flatten();
    let record = SupplyRecord {
        session: request.session,
        thread_slug: single_slug(&resolution),
        content_hash: block.as_ref().map(|b| b.hash.clone()),
        carrier: request.carrier,
        arm,
        resolution: resolution_name(&resolution),
        signals: signals_json(&resolution),
    };
    Supply {
        block,
        record: Some(record),
    }
}

fn single_slug(resolution: &Resolution) -> Option<String> {
    match resolution {
        Resolution::One(pick) => Some(pick.slug.clone()),
        Resolution::Several(_) | Resolution::None => None,
    }
}

fn resolution_name(resolution: &Resolution) -> &'static str {
    match resolution {
        Resolution::One(_) => "one",
        Resolution::Several(_) => "several",
        Resolution::None => "none",
    }
}

fn signals_json(resolution: &Resolution) -> Value {
    let picks = match resolution {
        Resolution::One(pick) => std::slice::from_ref(pick),
        Resolution::Several(picks) => picks.as_slice(),
        Resolution::None => &[],
    };
    Value::Array(
        picks
            .iter()
            .map(|pick| {
                json!({
                    "slug": pick.slug,
                    "score": pick.score,
                    "signals": pick.signals.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

#[cfg(test)]
#[path = "supply_tests.rs"]
mod tests;
