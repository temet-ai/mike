use chrono::{Duration, Utc};

use super::reads::{self, CauseGroup};
use crate::state::AppState;
use crate::web_base::capability::{CapabilityRow, CapabilityState};

const ID: &str = "prompt_cache";
const LABEL: &str = "Prompt cache";
const PAGE: &str = "/diagnostics/prompt-cache";
const WINDOW_HOURS: i64 = 24;
const TOP_STAGES: usize = 3;

fn row(state: CapabilityState, detail: String, fix: String) -> CapabilityRow {
    CapabilityRow {
        id: ID,
        label: LABEL,
        state,
        detail,
        fix,
    }
}

pub(crate) fn tokens(n: i64) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1_000_000.0),
        n if n >= 1_000 => format!("{}k", n / 1_000),
        n => n.to_string(),
    }
}

fn sum<'a>(groups: impl Iterator<Item = &'a CauseGroup>) -> (i64, i64) {
    groups.fold((0, 0), |(b, t), g| (b + g.breaks, t + g.tokens_rewritten))
}

fn by_stage(groups: &[CauseGroup]) -> Vec<(String, i64, i64)> {
    let mut stages: Vec<(String, i64, i64)> = Vec::new();
    for g in groups.iter().filter(|g| g.cause == "mike") {
        let name = g.stage.clone().unwrap_or_default();
        match stages.iter_mut().find(|(n, _, _)| *n == name) {
            Some(s) => {
                s.1 += g.breaks;
                s.2 += g.tokens_rewritten;
            }
            None => stages.push((name, g.breaks, g.tokens_rewritten)),
        }
    }
    stages.sort_by_key(|s| std::cmp::Reverse(s.2));
    stages
}

fn others(groups: &[CauseGroup]) -> String {
    let (client, client_tokens) = sum(groups.iter().filter(|g| g.cause == "client"));
    let (unknown, unknown_tokens) = sum(groups.iter().filter(|g| g.cause == "unknown"));
    let (expired, expired_tokens) = sum(groups.iter().filter(|g| g.kind == "expiry"));
    format!(
        "Claude Code's own changes caused {client} ({}), {unknown} had no cause found ({}), and {expired} resume(s) after the cache expired rewrote {}.",
        tokens(client_tokens),
        tokens(unknown_tokens),
        tokens(expired_tokens)
    )
}

pub(crate) fn health_row(groups: &[CauseGroup]) -> CapabilityRow {
    let stages = by_stage(groups);
    let (mike, mike_tokens) = sum(groups.iter().filter(|g| g.cause == "mike"));
    if mike == 0 {
        return row(
            CapabilityState::Active,
            format!(
                "No prompt-cache break caused by mike in the last {WINDOW_HOURS} hours. {}",
                others(groups)
            ),
            String::new(),
        );
    }
    let named = stages
        .iter()
        .take(TOP_STAGES)
        .map(|(s, b, t)| format!("{s} {b} ({})", tokens(*t)))
        .collect::<Vec<_>>()
        .join(", ");
    row(
        CapabilityState::Degraded,
        format!(
            "In the last {WINDOW_HOURS} hours mike's own edits broke the prompt cache {mike} time(s) and rewrote {} tokens: {named}. {}",
            tokens(mike_tokens),
            others(groups)
        ),
        format!(
            "Open {PAGE} to see each break, the request that sent the differing bytes and the stage that changed them; make that stage send the same bytes on Claude Code's full re-sends."
        ),
    )
}

pub(crate) async fn capability(state: &AppState) -> CapabilityRow {
    let Some(persist) = state.persist() else {
        return row(
            CapabilityState::Off,
            "Persistence is off, so prompt-cache breaks are not recorded".to_owned(),
            String::new(),
        );
    };
    let since = Utc::now() - Duration::hours(WINDOW_HOURS);
    match reads::groups(persist.pool(), None, since).await {
        Ok(groups) => health_row(&groups),
        Err(e) => row(
            CapabilityState::Degraded,
            format!("Could not read the prompt-cache breaks: {e}"),
            "Check that Postgres is reachable at the configured MIKE_DATABASE_URL".to_owned(),
        ),
    }
}

#[cfg(test)]
#[path = "health_tests.rs"]
mod tests;
