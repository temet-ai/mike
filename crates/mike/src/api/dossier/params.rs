use chrono::{DateTime, Utc};
use serde::Deserialize;

pub(crate) use crate::dossier::view::parse::parse_as_of;
use crate::dossier::view::parse::{AudienceArgs, parse_audience_parts};

pub(crate) const DEFAULT_BUDGET: usize = 24_000;
pub(crate) const DEFAULT_LIMIT: usize = 500;

pub(crate) const SECTIONS: [&str; 10] = [
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
pub(crate) struct DossierParams {
    pub(crate) as_of: Option<DateTime<Utc>>,
    pub(crate) as_of_turn: Option<i64>,
    pub(crate) mode: Option<String>,
    pub(crate) audience: Option<String>,
    pub(crate) turn_id: Option<i64>,
    pub(crate) slices: Option<String>,
    pub(crate) sections: Option<String>,
    pub(crate) budget_chars: Option<usize>,
    pub(crate) section: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
}

pub(crate) fn parse_audience(
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

pub(crate) fn check_section(section: Option<&str>) -> Result<(), String> {
    match section {
        Some(name) if !SECTIONS.contains(&name) => Err(format!("unknown section {name:?}")),
        _ => Ok(()),
    }
}
