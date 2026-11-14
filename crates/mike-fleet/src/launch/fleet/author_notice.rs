use crate::store::persistence::launch_repo::LaunchPersist as _;
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::questions::{blocked_waiting, digest_line};
use super::seated::Seated;
use super::status::{FleetStatus, ProblemStatus};
use super::told;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;
use crate::store::persistence::questions::QuestionRow;

const IDLE_ALERT_SECONDS: i64 = 600;
const IDLE_EPISODE_SECONDS: i64 = 600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub key: String,
    pub line: String,
}

fn problem_item(row: &FleetRow, problem: &ProblemStatus) -> Item {
    let task = problem
        .task
        .as_deref()
        .map(|task| format!(" on task {task}"))
        .unwrap_or_default();
    Item {
        key: format!(
            "author:problem:{}:{}:{}",
            problem.trigger,
            problem.task.as_deref().unwrap_or("-"),
            problem.opened_at.timestamp()
        ),
        line: format!(
            "Problem {}{task}: {}. mike's problem-solver escalated it to you. Settle it by changing the definition (define_fleet fleet={}) or mailing the seats, or end the run with `mike fleet stop {}`.",
            problem.trigger, problem.detail, row.fleet_uuid, row.slug
        ),
    }
}

fn question_item(question: &QuestionRow, status: &FleetStatus) -> Item {
    Item {
        key: format!("author:question:{}", question.question_uuid),
        line: digest_line(question, status.now),
    }
}

fn idle_item(row: &FleetRow, status: &FleetStatus) -> Option<Item> {
    let idle = status
        .every_seat_idle_seconds
        .filter(|s| *s >= IDLE_ALERT_SECONDS)?;
    let since = status.now.timestamp() - idle;
    Some(Item {
        key: format!("author:idle:{}", since.div_euclid(IDLE_EPISODE_SECONDS)),
        line: format!(
            "Nothing is moving: every open seat has been idle for {} min. Read `mike fleet status {}`, mail the seat that should move, or end the run with `mike fleet stop {}`.",
            idle / 60,
            row.slug,
            row.slug
        ),
    })
}

pub fn items(row: &FleetRow, status: &FleetStatus, questions: &[QuestionRow]) -> Vec<Item> {
    status
        .problems
        .iter()
        .filter(|problem| problem.escalated)
        .map(|problem| problem_item(row, problem))
        .chain(questions.iter().map(|q| question_item(q, status)))
        .chain(idle_item(row, status))
        .collect()
}

pub fn mail_text(row: &FleetRow, fresh: &[Item]) -> String {
    let lines: Vec<String> = fresh
        .iter()
        .map(|item| format!("- {}", item.line))
        .collect();
    format!(
        "[mike] Fleet {} needs you; nothing else will settle {}:\n{}",
        row.slug,
        if fresh.len() == 1 { "this" } else { "these" },
        lines.join("\n")
    )
}

pub async fn tell(persist: &PersistHandle, row: &FleetRow, status: &FleetStatus) {
    let (Some(author), Some(run), Ok(def)) = (
        row.created_by,
        row.run_id.as_deref(),
        FleetDefinition::from_value(&row.definition),
    ) else {
        return;
    };
    if Seated::of_run(persist, run, &def)
        .await
        .control_seated(&def)
    {
        return;
    }
    let questions = blocked_waiting(persist, run, status.now).await;
    let said = told::load(persist, run).await;
    let fresh: Vec<Item> = items(row, status, &questions)
        .into_iter()
        .filter(|item| !said.already(&item.key))
        .collect();
    if fresh.is_empty() {
        return;
    }
    let body = mail_text(row, &fresh);
    match persist
        .launch()
        .insert_message(Uuid::new_v4(), author, None, &body)
        .await
    {
        Ok(_) => {
            let keys: Vec<String> = fresh.into_iter().map(|item| item.key).collect();
            told::mark(persist, run, &keys).await;
        }
        Err(err) => {
            tracing::warn!(fleet = %row.slug, error = %err, "fleet: could not mail the author what is waiting for them");
        }
    }
}

#[cfg(test)]
#[path = "author_notice_tests.rs"]
mod tests;
