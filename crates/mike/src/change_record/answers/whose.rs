use serde::Serialize;

use super::caller::AnswerError;
use super::lines::{ChangeLine, line_of, names_for};
use crate::change_record::{ChangeEvent, ChangeKind};
use crate::store::persistence::PersistHandle;

const READ: i64 = 50;
const CHANGE_KINDS: [ChangeKind; 3] = [
    ChangeKind::Edit,
    ChangeKind::ShellChange,
    ChangeKind::Commit,
];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WhoseAnswer {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_change: Option<ChangeLine>,
    pub committed: bool,
}

pub async fn whose(
    persist: &PersistHandle,
    checkout: &str,
    path: &str,
) -> Result<WhoseAnswer, AnswerError> {
    let events = persist
        .latest_change_events(checkout, &[path.to_owned()], &CHANGE_KINDS, READ)
        .await?;
    let on_path: Vec<ChangeEvent> = events
        .into_iter()
        .filter(|e| e.paths.iter().any(|p| p == path))
        .collect();
    let last = on_path
        .iter()
        .find(|e| e.kind != ChangeKind::Commit)
        .cloned();
    let committed = match last.as_ref() {
        Some(change) => on_path
            .iter()
            .any(|e| e.kind == ChangeKind::Commit && e.id > change.id),
        None if !on_path.is_empty() => true,
        None => clean_at_head(checkout, path).await,
    };
    let names = names_for(persist, &last.iter().cloned().collect::<Vec<_>>()).await?;
    Ok(WhoseAnswer {
        path: path.to_owned(),
        last_change: last.as_ref().map(|e| line_of(e, &names)),
        committed,
    })
}

async fn clean_at_head(checkout: &str, path: &str) -> bool {
    let tracked = git_succeeds(checkout, &["ls-files", "--error-unmatch", "--", path]).await;
    tracked && git_stdout_is_empty(checkout, &["status", "--porcelain", "--", path]).await
}

async fn git_succeeds(checkout: &str, args: &[&str]) -> bool {
    tokio::process::Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args(args)
        .output()
        .await
        .is_ok_and(|out| out.status.success())
}

async fn git_stdout_is_empty(checkout: &str, args: &[&str]) -> bool {
    tokio::process::Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args(args)
        .output()
        .await
        .is_ok_and(|out| out.status.success() && out.stdout.is_empty())
}
