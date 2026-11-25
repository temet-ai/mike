use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::web_repo::WebPersist as _;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::store::persistence::wright::WaitingCause;
use crate::store::persistence::{PersistError, PersistHandle};
use crate::wright::conversation::notice::PAGE_URL;

pub(crate) const WAITED_HOURS: i32 = 6;
const LISTED: usize = 5;

fn cause_line(cause: &WaitingCause) -> String {
    let resource: String = cause.resource.chars().take(80).collect();
    format!(
        "- cause {id} ({check}, {problems} problem(s), seen {observations} times, open since {opened}, {attempts} attempt(s)): {resource}",
        id = cause.cause_id,
        check = cause.check_name,
        problems = cause.problems,
        observations = cause.observations,
        opened = cause.opened_at.format("%Y-%m-%d %H:%M UTC"),
        attempts = cause.attempts,
    )
}

pub(crate) fn notice(waiting: &[WaitingCause], limited: Option<DateTime<Utc>>) -> String {
    let mut out = format!(
        "[Wright] {n} cause(s) have waited over {WAITED_HOURS} hours with no finding, highest priority first:\n",
        n = waiting.len()
    );
    for cause in waiting.iter().take(LISTED) {
        out.push_str(&cause_line(cause));
        out.push('\n');
    }
    if waiting.len() > LISTED {
        out.push_str(&format!("- and {} more\n", waiting.len() - LISTED));
    }
    if let Some(until) = limited {
        out.push_str(&format!(
            "The engineer waits for the Claude limit until {} and then goes on by itself.\n",
            until.format("%Y-%m-%d %H:%M UTC")
        ));
    }
    out.push_str(&format!("Open {PAGE_URL} to see the queue."));
    out
}

async fn tell_operator(persist: &PersistHandle) -> Result<(), PersistError> {
    let Some(recipient) = persist.web().wright_operator_session().await? else {
        return Ok(());
    };
    let waiting = persist
        .web()
        .claim_overdue_wright_causes(WAITED_HOURS)
        .await?;
    if waiting.is_empty() {
        return Ok(());
    }
    let limited = persist
        .web()
        .wright_engineer_blocked_until()
        .await?
        .filter(|until| *until > Utc::now());
    persist
        .launch()
        .insert_message(Uuid::new_v4(), recipient, None, &notice(&waiting, limited))
        .await?;
    Ok(())
}

async fn step(persist: &PersistHandle) -> Result<(), PersistError> {
    let assigned = persist.web().assign_wright_causes().await?;
    if assigned.assigned > 0 {
        tracing::info!(
            problems = assigned.assigned,
            new_causes = assigned.opened,
            "wright triage: problems assigned to causes"
        );
    }
    tell_operator(persist).await
}

pub(crate) async fn run(persist: &PersistHandle) {
    if let Err(e) = step(persist).await {
        tracing::warn!(error = %e, "wright triage: the pass failed");
    }
}
