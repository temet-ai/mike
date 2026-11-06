use super::caller::{AnswerError, Caller};
use super::changes::{Scope, Since, changes_limited};
use super::lines::{how, short};
use crate::store::persistence::PersistHandle;

const TAIL_LINES: i64 = 11;
const TASK_READ: i64 = 200;

pub async fn task_lines(
    persist: &PersistHandle,
    caller: &Caller,
    task: Option<&str>,
) -> Result<Vec<String>, AnswerError> {
    let Some(task) = task else {
        return Ok(Vec::new());
    };
    let place = persist
        .change_place(caller.session_id, &caller.checkout)
        .await?;
    if place == 0 {
        return Ok(Vec::new());
    }
    let recent = persist
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
    let place = persist
        .change_place(caller.session_id, &caller.checkout)
        .await?;
    if place == 0 {
        let newest = persist
            .latest_change_events(&caller.checkout, &[], &[], 1)
            .await?;
        if let Some(first) = newest.first() {
            persist
                .move_change_place(caller.session_id, &caller.checkout, first.id)
                .await?;
        }
        return Ok(Vec::new());
    }
    let answer = changes_limited(persist, caller, &Since::Place, &Scope::Mine, TAIL_LINES).await?;
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
