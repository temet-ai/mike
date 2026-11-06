use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use super::ledger::{AgentEntry, BackgroundEntry, Ledger};

pub(crate) const STALLED_AFTER_MINUTES: i64 = 15;
pub(crate) const LANDED_POLLING_AFTER_MINUTES: i64 = 10;
pub(crate) const MAIN_IDLE_AFTER_MINUTES: i64 = 15;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Attention {
    pub kind: &'static str,
    pub subject: String,
    pub since: DateTime<Utc>,
    pub message: String,
}

pub(crate) struct Clock {
    pub now: DateTime<Utc>,
    pub main_last_request_at: Option<DateTime<Utc>>,
}

impl Clock {
    fn idle_since(&self, last: Option<DateTime<Utc>>, minutes: i64) -> bool {
        last.is_some_and(|t| t < self.now - Duration::minutes(minutes))
    }
}

pub(crate) fn assess(ledger: &Ledger, clock: &Clock) -> Vec<Attention> {
    let mut found = Vec::new();
    for agent in &ledger.agents {
        found.extend(agent_attention(agent, clock));
    }
    for job in &ledger.background {
        found.extend(job_attention(job, ledger, clock));
    }
    found.extend(main_idle_attention(ledger, clock));
    found
}

pub(crate) fn next_change(ledger: &Ledger, clock: &Clock) -> Option<DateTime<Utc>> {
    let stalled = Duration::minutes(STALLED_AFTER_MINUTES);
    let idle = Duration::minutes(MAIN_IDLE_AFTER_MINUTES);
    let landed = Duration::minutes(LANDED_POLLING_AFTER_MINUTES);
    let mut at: Vec<DateTime<Utc>> = Vec::new();
    for agent in &ledger.agents {
        let last = agent.last_request_at.unwrap_or(agent.launched_at);
        at.extend([last + stalled, last + idle]);
        at.extend(agent.git.pushes.iter().map(|push| push.at + landed));
    }
    if let Some(main) = clock.main_last_request_at {
        at.extend([main + stalled, main + idle]);
    }
    at.into_iter().filter(|t| *t >= clock.now).min()
}

fn label(description: Option<&String>) -> String {
    description.map(|d| format!(" ({d})")).unwrap_or_default()
}

fn owner_name(owner: Option<&String>) -> String {
    owner.map_or_else(|| "the main thread".to_string(), |o| format!("agent {o}"))
}

fn agent_attention(agent: &AgentEntry, clock: &Clock) -> Option<Attention> {
    let what = format!(
        "Agent {}{}",
        agent.agent_id,
        label(agent.description.as_ref())
    );
    let last = agent.last_request_at.unwrap_or(agent.launched_at);
    match agent.state.as_str() {
        "orphaned" => Some(Attention {
            kind: "result_lost",
            subject: agent.agent_id.clone(),
            since: last,
            message: format!(
                "{what} was still working when {} ended, so its result reached no one. \
                 Read its output file or re-run it.",
                owner_name(agent.dispatched_by.as_ref())
            ),
        }),
        "landed_polling" => landed_polling(agent, &what, clock),
        "running" if clock.idle_since(Some(last), STALLED_AFTER_MINUTES) => Some(Attention {
            kind: "agent_silent",
            subject: format!("{}@{}", agent.agent_id, last.timestamp()),
            since: last,
            message: format!(
                "{what} is marked running but has made no request since {last}. \
                 Check whether it is blocked or should be stopped."
            ),
        }),
        _ => None,
    }
}

fn landed_polling(agent: &AgentEntry, what: &str, clock: &Clock) -> Option<Attention> {
    let last_push = agent.git.pushes.iter().max_by_key(|p| p.at)?;
    if !clock.idle_since(Some(last_push.at), LANDED_POLLING_AFTER_MINUTES) {
        return None;
    }
    Some(Attention {
        kind: "landed_polling",
        subject: format!("{}@{}", agent.agent_id, last_push.at.timestamp()),
        since: last_push.at,
        message: format!(
            "{what} pushed {} to {} at {} and is still running without editing anything. \
             Its work has landed; it is probably only polling.",
            last_push.range, last_push.target, last_push.at
        ),
    })
}

fn owner_last_request(
    owner: Option<&String>,
    ledger: &Ledger,
    clock: &Clock,
) -> Option<DateTime<Utc>> {
    match owner {
        None => clock.main_last_request_at,
        Some(id) => ledger
            .agents
            .iter()
            .find(|a| &a.agent_id == id)
            .and_then(|a| a.last_request_at),
    }
}

fn job_attention(job: &BackgroundEntry, ledger: &Ledger, clock: &Clock) -> Option<Attention> {
    let what = format!(
        "Background job {}{}",
        job.task_id,
        label(job.description.as_ref())
    );
    match job.state.as_str() {
        "orphaned" => Some(Attention {
            kind: "result_lost",
            subject: job.task_id.clone(),
            since: job.launched_at,
            message: format!(
                "{what} was still running when {} ended, so its result reached no one.",
                owner_name(job.owner_agent_id.as_ref())
            ),
        }),
        "running" => {
            let owner_last = owner_last_request(job.owner_agent_id.as_ref(), ledger, clock);
            if !clock.idle_since(owner_last, STALLED_AFTER_MINUTES) {
                return None;
            }
            let since = owner_last.unwrap_or(job.launched_at);
            Some(Attention {
                kind: "stalled_wait",
                subject: format!("{}@{}", job.task_id, since.timestamp()),
                since,
                message: format!(
                    "{what} is still running and {} has been idle since {since}. \
                     If the job is hung, stop it; if it finished, its notification never arrived.",
                    owner_name(job.owner_agent_id.as_ref())
                ),
            })
        }
        _ => None,
    }
}

fn main_idle_attention(ledger: &Ledger, clock: &Clock) -> Option<Attention> {
    let open = ledger.running_agents + ledger.running_background;
    if open == 0 || !clock.idle_since(clock.main_last_request_at, MAIN_IDLE_AFTER_MINUTES) {
        return None;
    }
    let every_open_agent_silent = ledger
        .agents
        .iter()
        .filter(|a| a.state == "running")
        .all(|a| {
            clock.idle_since(
                Some(a.last_request_at.unwrap_or(a.launched_at)),
                MAIN_IDLE_AFTER_MINUTES,
            )
        });
    if !every_open_agent_silent {
        return None;
    }
    let since = clock.main_last_request_at?;
    Some(Attention {
        kind: "main_idle_with_open_work",
        subject: format!(
            "main@{}#{}a{}j",
            since.timestamp(),
            ledger.running_agents,
            ledger.running_background
        ),
        since,
        message: format!(
            "The main thread has been idle since {since} while {} agent(s) and {} background \
             job(s) are still open. Check the ledger and decide what to resume or stop.",
            ledger.running_agents, ledger.running_background
        ),
    })
}
