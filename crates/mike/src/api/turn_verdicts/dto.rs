use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::store::persistence::operator_turns::projection_reads::LegacyLabelRow;
use crate::store::persistence::turn_verdicts::TurnVerdictRow;

#[derive(Debug, Deserialize)]
pub(crate) struct TurnVerdictsQuery {
    pub(crate) mode: Option<String>,
    pub(crate) limit: Option<i64>,
    pub(crate) offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TurnVerdictListQuery {
    pub(crate) mode: Option<String>,
    pub(crate) bundle_id: Option<i64>,
    pub(crate) rubric_version: Option<String>,
    pub(crate) session_id: Option<uuid::Uuid>,
    pub(crate) since: Option<DateTime<Utc>>,
    pub(crate) until: Option<DateTime<Utc>>,
    pub(crate) limit: Option<i64>,
    pub(crate) offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TurnVerdictDto {
    #[serde(flatten)]
    pub(crate) row: TurnVerdictRow,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) is_live_projection: Option<bool>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TurnVerdictsPage {
    pub(crate) turn_id: i64,
    pub(crate) live_verdict_id: Option<i64>,
    pub(crate) legacy_label: Option<LegacyLabelRow>,
    pub(crate) verdicts: Vec<TurnVerdictDto>,
    pub(crate) total: i64,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct TurnVerdictListPage {
    pub(crate) items: Vec<TurnVerdictDto>,
    pub(crate) total: i64,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
}

pub(crate) fn dtos(rows: Vec<TurnVerdictRow>) -> Vec<TurnVerdictDto> {
    rows.into_iter()
        .map(|row| TurnVerdictDto {
            row,
            is_live_projection: None,
        })
        .collect()
}

pub(crate) fn turn_dtos(
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
