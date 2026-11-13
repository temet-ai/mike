pub mod reads;
pub mod writes;

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::super::PersistError;
use crate::wire::composition::{Composition, CompositionMode};

#[derive(sqlx::FromRow)]
pub struct CompositionRow {
    pub id: i64,
    pub uuid: uuid::Uuid,
    pub prompt_sha256: String,
    pub label: String,
    pub blocks: Value,
    pub mode: String,
    pub lane: String,
    pub notes: Option<String>,
    pub agent_id: Option<i64>,
    pub agent_name: Option<String>,
    pub agent_definition_path: Option<String>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<CompositionRow> for Composition {
    type Error = PersistError;
    fn try_from(row: CompositionRow) -> Result<Self, Self::Error> {
        let mode = CompositionMode::parse_str(&row.mode).map_err(|err| {
            PersistError::DataInvariant(format!(
                "prompt_compositions row for prompt_sha256={:?}: {err}",
                row.prompt_sha256
            ))
        })?;
        let blocks: Vec<Value> = serde_json::from_value(row.blocks).map_err(|err| {
            PersistError::DataInvariant(format!(
                "prompt_compositions row for prompt_sha256={:?}: failed to decode blocks JSON: {err}",
                row.prompt_sha256
            ))
        })?;
        Ok(Composition {
            id: row.id,
            uuid: row.uuid,
            prompt_sha256: row.prompt_sha256,
            label: row.label,
            blocks,
            mode,
            lane: row.lane,
            notes: row.notes,
            agent_id: row.agent_id,
            agent_name: row.agent_name,
            agent_definition_path: row.agent_definition_path,
            version: row.version,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct CompositionRevisionRow {
    id: i64,
    uuid: uuid::Uuid,
    composition_id: i64,
    composition_uuid: uuid::Uuid,
    version: i64,
    label: String,
    blocks: Value,
    mode: String,
    notes: Option<String>,
    created_at: DateTime<Utc>,
}

pub struct NewCompositionParams<'a> {
    pub prompt_sha256: &'a str,
    pub label: &'a str,
    pub blocks: &'a Value,
    pub mode: Option<CompositionMode>,
    pub agent_name: Option<&'a str>,
    pub agent_definition_path: Option<&'a str>,
}

struct OwnedNewComposition {
    prompt_sha256: String,
    label: String,
    blocks: Value,
    mode_str: &'static str,
    agent_name: Option<String>,
    agent_definition_path: Option<String>,
    lane: &'static str,
}

#[derive(Debug, Default)]
pub struct PatchCompositionParams<'a> {
    pub label: Option<&'a str>,
    pub blocks: Option<&'a Value>,
    pub mode: Option<CompositionMode>,
    pub notes: Option<Option<&'a str>>,
    pub agent_name: Option<Option<&'a str>>,
    pub agent_definition_path: Option<Option<&'a str>>,
}

#[derive(Debug)]
pub enum DeleteOutcome {
    Removed,
    NotFound,
    StillReferenced { count: i64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompositionRevision {
    pub id: i64,
    pub uuid: uuid::Uuid,
    pub composition_id: i64,
    pub composition_uuid: uuid::Uuid,
    pub version: i64,
    pub label: String,
    pub blocks: Vec<Value>,
    pub mode: CompositionMode,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}
