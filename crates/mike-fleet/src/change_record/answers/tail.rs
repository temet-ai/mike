use super::caller::{AnswerError, Caller};
use super::changes::{Scope, Since, changes_from_place};
use super::lines::{how, short};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet_repo::FleetPersist as _;

const TAIL_LINES: i64 = 11;
const TASK_READ: i64 = 200;

fn nothing_past(place: i64, newest: Option<i64>) -> bool {
    newest.is_none_or(|newest| newest <= place)
}

async fn place_with_news(
    persist: &PersistHandle,
    caller: &Caller,
) -> Result<Option<i64>, AnswerError> {
    let place = persist
        .fleet()
        .change_place(caller.session_id, &caller.checkout)
        .await?;
    if place == 0 {
        return Ok(Some(0));
    }
    let newest = persist.fleet().newest_change_event(&caller.checkout).await?;
    Ok((!nothing_past(place, newest)).then_some(place))
}

pub async fn task_lines(
    persist: &PersistHandle,
    caller: &Caller,
    task: Option<&str>,
) -> Result<Vec<String>, AnswerError> {
    let Some(task) = task else {
        return Ok(Vec::new());
    };
    let Some(place) = place_with_news(persist, caller).await? else {
        return Ok(Vec::new());
    };
    if place == 0 {
        return Ok(Vec::new());
    }
    let recent = persist
        .fleet()
        .latest_change_events(&caller.checkout, &[], &[], TASK_READ)
        .await?;
    Ok(recent
        .iter()
        .filter(|e| e.id > place && e.task.as_deref() == Some(task) && e.paths.is_empty())
        .take(TAIL_LINES as usize)
        .map(|e| format!("{} {task}: {}", how(e.kind), e.subject))
        .collect())
}

pub async fn tail(persist: &PersistHandle, caller: &Caller) -> Result<Vec<String>, AnswerError> {
    let Some(place) = place_with_news(persist, caller).await? else {
        return Ok(Vec::new());
    };
    if place == 0 {
        if let Some(first) = persist.fleet().newest_change_event(&caller.checkout).await? {
            persist
                .fleet()
                .move_change_place(caller.session_id, &caller.checkout, first)
                .await?;
        }
        return Ok(Vec::new());
    }
    let answer = changes_from_place(
        persist,
        caller,
        &Since::Place,
        &Scope::Mine,
        TAIL_LINES,
        place,
    )
    .await?;
    let mut lines: Vec<String> = answer
        .lines
        .iter()
        .filter(|line| line.who != super::lines::NO_AUTHOR)
        .map(short)
        .collect();
    if answer.more && !lines.is_empty() {
        lines.push("more changes wait: ask what_changed".to_owned());
    }
    Ok(lines)
}

#[cfg(test)]
#[path = "tail_tests.rs"]
mod tail_tests;
