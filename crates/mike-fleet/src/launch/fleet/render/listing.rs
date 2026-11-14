use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::clock::day_and_time;
use super::ink::{DIM, FAINT, INK, RED, fit, fleet_state_colour, pad, paint};
use crate::launch::fleet::checks::Missing;

const SLUG_WIDTH: usize = 32;
const STATE_WIDTH: usize = 10;
const MISSING_WIDTH: usize = 14;
const CREATED_WIDTH: usize = 18;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FleetListing {
    pub fleet: String,
    pub slug: String,
    pub project_path: String,
    pub state: String,
    pub run_id: Option<String>,
    pub missing: Vec<Missing>,
    pub created_at: DateTime<Utc>,
}

pub fn render_list(fleets: &[FleetListing], width: usize) -> String {
    if fleets.is_empty() {
        return paint("no fleets yet", FAINT, false);
    }
    let heading = paint(
        &format!(
            "{}{}{}{}run",
            pad("fleet", SLUG_WIDTH),
            pad("state", STATE_WIDTH),
            pad("missing", MISSING_WIDTH),
            pad("created", CREATED_WIDTH),
        ),
        DIM,
        false,
    );
    let mut lines = vec![heading];
    lines.extend(fleets.iter().map(row));
    lines
        .iter()
        .map(|line| fit(line, width))
        .collect::<Vec<_>>()
        .join("\n")
}

fn row(fleet: &FleetListing) -> String {
    let missing = match fleet.missing.len() {
        0 => "none missing".to_string(),
        n => format!("{n} missing"),
    };
    let missing_colour = if fleet.missing.is_empty() { DIM } else { RED };
    format!(
        "{}{}{}{}{}",
        pad(&paint(&fleet.slug, INK, true), SLUG_WIDTH),
        pad(
            &paint(&fleet.state, fleet_state_colour(&fleet.state), false),
            STATE_WIDTH
        ),
        pad(&paint(&missing, missing_colour, false), MISSING_WIDTH),
        pad(
            &paint(&day_and_time(fleet.created_at), DIM, false),
            CREATED_WIDTH
        ),
        paint(fleet.run_id.as_deref().unwrap_or("no run yet"), DIM, false)
    )
}
