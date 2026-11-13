use crate::store::persistence::fleet_repo::FleetPersist as _;
use serde_json::{Value, json};
use tokio::process::Command;

use super::ChangesError;
use super::resolve::{FleetSeat, Resolved};
use crate::change_record::answers;
use crate::store::persistence::PersistHandle;

pub(super) async fn standing(
    persist: &PersistHandle,
    resolved: &Resolved,
) -> Result<Value, ChangesError> {
    let caller = resolved.caller()?;
    let answer = answers::standing(persist, &caller).await?;
    let mut value =
        serde_json::to_value(answer).map_err(|e| ChangesError::Invalid(e.to_string()))?;
    if let Some(seat) = &resolved.fleet {
        value["fleet"] = fleet_part(persist, seat).await?;
    }
    Ok(value)
}

async fn fleet_part(persist: &PersistHandle, seat: &FleetSeat) -> Result<Value, ChangesError> {
    let repo = seat.definition.repo.as_str();
    let state = seat
        .definition
        .tasks
        .get(&seat.task)
        .and_then(|t| t.state.clone());
    let head = git_line(repo, &["rev-parse", "HEAD"]).await;
    let upstream = git_line(repo, &["rev-parse", "@{upstream}"]).await;
    let counts = git_line(
        repo,
        &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
    )
    .await
    .and_then(|text| ahead_behind(&text));
    let check = persist.fleet().latest_tree_check(&seat.run).await?;
    let latest_check = check.map(|row| {
        json!({
            "command": row.command,
            "started_at": row.started_at,
            "finished_at": row.finished_at,
            "passed": row.finished_at.map(|_| row.exit_code == Some(0)),
            "running": row.finished_at.is_none(),
            "tree_moved_while_it_ran": row.moved(),
        })
    });
    Ok(json!({
        "task": seat.task,
        "seat": seat.seat,
        "state": state,
        "branch": seat.definition.branch,
        "head": head,
        "upstream": upstream,
        "ahead": counts.map(|c| c.0),
        "behind": counts.map(|c| c.1),
        "latest_check": latest_check,
    }))
}

fn ahead_behind(text: &str) -> Option<(u64, u64)> {
    let mut parts = text.split_whitespace();
    let ahead = parts.next()?.parse().ok()?;
    let behind = parts.next()?.parse().ok()?;
    Some((ahead, behind))
}

async fn git_line(repo: &str, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let line = text.trim();
    (!line.is_empty()).then(|| line.to_owned())
}

#[cfg(test)]
mod tests {
    use super::ahead_behind;

    #[test]
    fn ahead_and_behind_are_read_from_the_tab_separated_pair() {
        assert_eq!(ahead_behind("3\t1"), Some((3, 1)));
        assert_eq!(ahead_behind("x\t1"), None);
        assert_eq!(ahead_behind(""), None);
    }
}
