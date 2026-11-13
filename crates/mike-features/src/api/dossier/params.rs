use chrono::{DateTime, Utc};
use serde::Deserialize;

pub use crate::dossier::view::parse::parse_as_of;
use crate::dossier::view::parse::{AudienceArgs, parse_audience_parts};

pub const DEFAULT_BUDGET: usize = 24_000;
pub const DEFAULT_LIMIT: usize = 500;

pub const SECTIONS: [&str; 10] = [
    "session",
    "stream",
    "turns",
    "asks",
    "directives",
    "promises",
    "open_turn",
    "corrections",
    "reactions",
    "exclusions",
];

#[derive(Debug, Default, Deserialize)]
pub struct DossierParams {
    pub as_of: Option<DateTime<Utc>>,
    pub as_of_turn: Option<i64>,
    pub mode: Option<String>,
    pub audience: Option<String>,
    pub turn_id: Option<i64>,
    pub slices: Option<String>,
    pub sections: Option<String>,
    pub budget_chars: Option<usize>,
    pub section: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

pub fn parse_audience(
    params: &DossierParams,
) -> Result<
    Option<(
        crate::dossier::view::types::Audience,
        crate::dossier::view::types::Budget,
    )>,
    String,
> {
    parse_audience_parts(&AudienceArgs {
        audience: params.audience.as_deref().unwrap_or("none"),
        turn_id: params.turn_id,
        slices: params.slices.as_deref(),
        sections: params.sections.as_deref(),
        budget_chars: params.budget_chars.unwrap_or(DEFAULT_BUDGET),
    })
}

pub fn check_section(section: Option<&str>) -> Result<(), String> {
    match section {
        Some(name) if !SECTIONS.contains(&name) => Err(format!("unknown section {name:?}")),
        _ => Ok(()),
    }
}
