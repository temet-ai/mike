use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::store::persistence::operator_turns::projection_reads::LegacyLabelRow;
use crate::store::persistence::turn_verdicts::TurnVerdictRow;

#[derive(Debug, Deserialize)]
pub struct TurnVerdictsQuery {
    pub mode: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct TurnVerdictListQuery {
    pub mode: Option<String>,
    pub bundle_id: Option<i64>,
    pub rubric_version: Option<String>,
    pub session_id: Option<uuid::Uuid>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TurnVerdictDto {
    #[serde(flatten)]
    pub row: TurnVerdictRow,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_live_projection: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct TurnVerdictsPage {
    pub turn_id: i64,
    pub live_verdict_id: Option<i64>,
    pub legacy_label: Option<LegacyLabelRow>,
    pub verdicts: Vec<TurnVerdictDto>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Serialize)]
pub struct TurnVerdictListPage {
    pub items: Vec<TurnVerdictDto>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

pub fn dtos(rows: Vec<TurnVerdictRow>) -> Vec<TurnVerdictDto> {
    rows.into_iter()
        .map(|row| TurnVerdictDto {
            row,
            is_live_projection: None,
        })
        .collect()
}

pub fn turn_dtos(
    rows: Vec<TurnVerdictRow>,
    live_verdict_id: Option<i64>,
) -> Vec<TurnVerdictDto> {
    rows.into_iter()
        .map(|row| TurnVerdictDto {
            is_live_projection: Some(Some(row.id) == live_verdict_id),
            row,
        })
        .collect()
}
