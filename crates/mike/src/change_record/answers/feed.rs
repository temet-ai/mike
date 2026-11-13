use crate::store::persistence::fleet_repo::FleetPersist as _;
use serde::Serialize;

use super::caller::AnswerError;
use super::lines::{ChangeLine, line_of, names_for};
use super::remote::on_a_remote;
use crate::change_record::{ChangeEvent, ChangeKind};
use crate::store::persistence::PersistHandle;

pub const FEED_LINES: i64 = 50;

#[derive(Debug, Serialize)]
pub struct FeedAnswer {
    pub lines: Vec<ChangeLine>,
}

pub async fn feed(
    persist: &PersistHandle,
    checkout: &str,
    limit: i64,
) -> Result<FeedAnswer, AnswerError> {
    let events = persist
        .fleet()
        .latest_change_events(checkout, &[], &[], limit)
        .await?;
    let names = names_for(persist, &events).await?;
    let shas: Vec<String> = events.iter().filter_map(commit_sha).collect();
    let pushed = on_a_remote(checkout, &shas).await;
    let lines = events
        .iter()
        .map(|e| {
            let mut line = line_of(e, &names);
            line.pushed = commit_sha(e).is_some_and(|sha| pushed.contains(&sha));
            line
        })
        .collect();
    Ok(FeedAnswer { lines })
}

fn commit_sha(event: &ChangeEvent) -> Option<String> {
    if event.kind != ChangeKind::Commit {
        return None;
    }
    event
        .after_sha
        .clone()
        .or_else(|| event.detail["sha"].as_str().map(str::to_owned))
}

#[cfg(test)]
#[path = "feed_tests.rs"]
mod tests;
