use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, TimeDelta, Utc};

use crate::store::persistence::PersistHandle;

mod exposures;
mod fetch;
mod findings;
mod output;
mod promotion;
mod queries;
mod render;
mod render_activity;
mod render_quality;
mod report;
mod rows;
mod tool_reuse;

#[cfg(test)]
mod fetch_tests;
#[cfg(test)]
mod findings_tests;
#[cfg(test)]
mod output_tests;
#[cfg(test)]
mod promotion_tests;
#[cfg(test)]
mod render_tests;
#[cfg(test)]
mod tool_reuse_tests;

use fetch::Window;
use findings::Finding;
use promotion::{PromotionState, publication_status};
use report::Report;

const WINDOW_DAYS: i64 = 7;

#[derive(Debug, Clone)]
pub struct AuditRequest {
    pub end: DateTime<Utc>,
    pub output: PathBuf,
    pub promotion_dir: PathBuf,
    pub tool_reuse_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct AuditSummary {
    pub report: PathBuf,
    pub findings: usize,
    pub reuse_candidates: usize,
}

pub async fn run(handle: &PersistHandle, request: &AuditRequest) -> Result<AuditSummary> {
    let window = Window {
        start: request.end - TimeDelta::days(WINDOW_DAYS),
        end: request.end,
    };
    std::fs::create_dir_all(&request.output)
        .with_context(|| format!("cannot create {}", request.output.display()))?;
    let output = request
        .output
        .canonicalize()
        .with_context(|| format!("cannot resolve {}", request.output.display()))?;
    let previous = output::read_previous(&output);
    let promotion = PromotionState::load(&request.promotion_dir)?;
    let data = fetch::run_queries(handle.pool(), &window).await?;
    let mut report = Report::build(&window, data, &promotion);
    report.generated_at = Utc::now().to_rfc3339();
    report.publication = publication_status(&request.promotion_dir)?;
    if report.publication.state != "published" {
        report.findings.push(Finding::new(
            &format!("memory_publication_{}", report.publication.state),
            "global",
            serde_json::to_value(&report.publication).ok(),
        ));
    }
    report.exposures = exposures::recorded(handle, &window).await?;
    report.previous_window = previous.map(|earlier| earlier.against(&report.totals));
    report.reuse_events = tool_reuse::events_in(&request.tool_reuse_dir, &window)?;
    let rendered = render::render(&report, &promotion);
    let written = output::write_report(&output, request.end, &report, &rendered)?;
    tool_reuse::export(&request.tool_reuse_dir, &output.join("tool-recipes.md"))?;
    Ok(AuditSummary {
        report: written,
        findings: report.findings.len(),
        reuse_candidates: report.opportunities.len(),
    })
}
