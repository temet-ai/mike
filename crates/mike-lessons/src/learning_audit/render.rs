use serde_json::Value;

use super::delivery;
use super::findings::generated_skill_loads;
use super::promotion::PromotionState;
use super::render_activity::{render_memory_grammar, render_proposer_days, render_skill_live};
use super::render_quality::{render_lesson_quality, render_placement};
use super::report::Report;
use super::rows::{int, rows, show, text};

fn header(report: &Report) -> Vec<String> {
    let totals = &report.totals;
    let mut lines = vec![
        "# Weekly Mike learning audit".to_owned(),
        String::new(),
        format!(
            "Window: {} inclusive to {} exclusive.",
            report.start, report.end
        ),
        String::new(),
        format!(
            "{} sessions; {} requests; {} tool calls; {} requests lack input usage.",
            totals.sessions, totals.requests, totals.tool_calls, totals.missing_usage
        ),
        String::new(),
        concat!(
            "Input, output, cache-read and cache-write counts are reported separately in report.json. ",
            "Repeated results are candidates, not guaranteed savings; accepted labels are not independent proof of task completion."
        )
        .to_owned(),
        String::new(),
        "## Learning health".to_owned(),
        String::new(),
    ];
    if report.findings.is_empty() {
        lines.push("No learning-health findings.".to_owned());
    }
    lines.extend(
        report
            .findings
            .iter()
            .map(|finding| format!("- {}: {}", finding.project, finding.kind)),
    );
    lines
}

fn publication_line(report: &Report) -> String {
    let publication = &report.publication;
    let count = |value: &Option<Value>| {
        value
            .as_ref()
            .map_or_else(|| "0".to_owned(), |v| show(Some(v)))
    };
    format!(
        "- Memory/skill publication: `{}`; {} lessons, {} projects, {} installations; updated {}.",
        publication.state,
        count(&publication.lessons),
        count(&publication.memory_projects),
        publication.skill_installations.unwrap_or(0),
        publication.updated_at.as_deref().unwrap_or("unknown"),
    )
}

fn skill_load_lines(data: &Value, promotion: &PromotionState) -> Vec<String> {
    let loads = generated_skill_loads(data, promotion);
    let generated: Vec<_> = loads.iter().filter(|load| load.generated).collect();
    let mut lines = vec![format!(
        "- Skill loads this window: {} total, {} of mike-generated skills.",
        loads.iter().map(|load| load.loads).sum::<i64>(),
        generated.iter().map(|load| load.loads).sum::<i64>(),
    )];
    lines.extend(generated.iter().map(|load| {
        format!(
            "  - `{}`: {} loads across {} sessions.",
            load.skill, load.loads, load.sessions
        )
    }));
    lines
}

fn pipeline(report: &Report, promotion: &PromotionState) -> Vec<String> {
    let data = &report.data;
    let mut lines = vec![
        String::new(),
        "## Current learning pipeline".to_owned(),
        String::new(),
        concat!(
            "These are current operational snapshots, separate from the historical activity window. ",
            "Reserved attempts include failures and uncertain interrupted calls; they are not paid-call counts."
        )
        .to_owned(),
        String::new(),
    ];
    for row in rows(data, "proposal_jobs") {
        lines.push(format!(
            "- Proposal jobs `{}`: {}; oldest {}; maximum retries {}.",
            text(row, "status"),
            text(row, "jobs"),
            text(row, "oldest_created_at"),
            text(row, "max_retries"),
        ));
    }
    for row in rows(data, "proposal_recovery") {
        lines.push(format!(
            "- Historical recovery `{}`: {}.",
            text(row, "recovery_state"),
            text(row, "jobs"),
        ));
    }
    for row in rows(data, "proposer_budget") {
        lines.push(format!(
            "- UTC {}: {} reserved attempts; {} recorded proposer calls.",
            text(row, "day"),
            text(row, "consumed"),
            text(row, "recorded_calls"),
        ));
    }
    lines.extend(render_proposer_days(data));
    lines.push(publication_line(report));
    lines.extend(skill_load_lines(data, promotion));
    lines
}

fn governor_lines(data: &Value) -> Vec<String> {
    let mut lines = vec![
        String::new(),
        "Stored governor overrides (omitted settings use the running version’s defaults):"
            .to_owned(),
        String::new(),
    ];
    lines.extend(
        rows(data, "governor_settings")
            .iter()
            .map(|row| format!("- `{}`: `{}`", text(row, "key"), show(row.get("value")))),
    );
    lines
}

fn project_lines(report: &Report) -> Vec<String> {
    let mut lines = vec![
        String::new(),
        "## Projects".to_owned(),
        String::new(),
        "| Project | Sessions | Requests | Tool calls | Errors |".to_owned(),
        "|---|---:|---:|---:|---:|".to_owned(),
    ];
    lines.extend(report.projects.iter().map(|(name, row)| {
        format!(
            "| {name} | {} | {} | {} | {} |",
            row.sessions, row.requests, row.tool_calls, row.tool_errors
        )
    }));
    lines
}

fn guidance_lines() -> Vec<String> {
    vec![
        String::new(),
        "## Reuse opportunities".to_owned(),
        String::new(),
        concat!(
            "Full ranked candidates and stable source request IDs are in opportunities.json. ",
            "Discover validated recipes with `scripts/tool-reuse list`; use `show ID` for contract and `run ID` for a fresh-or-reused result. ",
            "Never execute a captured command merely because it repeated."
        )
        .to_owned(),
        String::new(),
        "## Follow-through".to_owned(),
        String::new(),
        concat!(
            "Compare with the prior window in report.json. Fix ingestion/delivery failures first. ",
            "For each repeated investigation, inspect its source, search the recipe registry, validate a reusable command, ",
            "and measure its hit/miss/error rate now by replaying the captured repeats against it. Judge quality using relevant subsequent tasks, not project-wide correlation."
        )
        .to_owned(),
    ]
}

fn evidence_lines(report: &Report) -> Vec<String> {
    let mut lines = vec![
        String::new(),
        "## Repeated tool/result evidence".to_owned(),
        String::new(),
        "| Project | Tool | Calls / sessions | Repeated bytes | Source request / tool |".to_owned(),
        "|---|---|---:|---:|---|".to_owned(),
    ];
    lines.extend(report.opportunities.iter().map(|item| {
        format!(
            "| {} | {} | {} / {} | {} | {} / {} |",
            text(item, "project"),
            text(item, "tool_name"),
            text(item, "calls"),
            text(item, "sessions"),
            text(item, "repeated_result_bytes"),
            text(item, "example_request_id"),
            text(item, "example_tool_use_id"),
        )
    }));
    lines.extend([
        String::new(),
        "## Helper creation across sessions".to_owned(),
        String::new(),
    ]);
    lines.extend(rows(&report.data, "created_tools").iter().map(|item| {
        format!(
            "- {}: `{}`, {} writes across {} sessions; source request {}. Inspect whether these were necessary edits or recreation.",
            text(item, "project"),
            text(item, "file_path"),
            int(item, "writes"),
            int(item, "sessions"),
            text(item, "example_request_id"),
        )
    }));
    lines
}

pub(super) fn render(report: &Report, promotion: &PromotionState) -> String {
    let mut lines = header(report);
    lines.extend(pipeline(report, promotion));
    lines.extend(render_memory_grammar(&report.data));
    lines.extend(render_lesson_quality(&report.data));
    lines.extend(render_placement(&report.data, promotion));
    if let Some(comparison) = &report.lesson_delivery {
        lines.extend(delivery::render(comparison));
    }
    lines.extend(render_skill_live(&report.data));
    lines.extend(governor_lines(&report.data));
    lines.extend(project_lines(report));
    lines.extend(guidance_lines());
    lines.extend(evidence_lines(report));
    lines.join("\n") + "\n"
}
