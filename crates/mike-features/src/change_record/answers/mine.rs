use crate::store::persistence::fleet_repo::FleetPersist as _;
use chrono::{DateTime, Utc};
use serde::Serialize;

use super::caller::{AnswerError, Caller};
use super::lines::{ChangeLine, line_of, names_for};
use crate::change_record::{ChangeEvent, ChangeKind};
use crate::store::persistence::PersistHandle;

const RECENT_EVENTS: i64 = 200;
const EVIDENCE_EVENTS: i64 = 20;
const CHECK_EVENTS: i64 = 5;
const CHANGE_KINDS: [ChangeKind; 2] = [ChangeKind::Edit, ChangeKind::ShellChange];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckVerdict {
    Passed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CheckAfterLastChange {
    pub ran: bool,
    pub verdict: Option<CheckVerdict>,
    pub last_change_at: Option<DateTime<Utc>>,
    pub checked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MinePath {
    pub path: String,
    pub last_change_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MineAnswer {
    pub uncommitted: Vec<MinePath>,
    pub check: CheckAfterLastChange,
    pub evidence: Vec<ChangeLine>,
}

pub async fn mine(persist: &PersistHandle, caller: &Caller) -> Result<MineAnswer, AnswerError> {
    let open = persist.fleet().open_changes(&caller.checkout).await?;
    let mut uncommitted: Vec<MinePath> = open
        .iter()
        .filter(|o| o.session_id == Some(caller.session_id))
        .map(|o| MinePath {
            path: o.path.clone(),
            last_change_at: o.at,
        })
        .collect();
    uncommitted.sort_by(|a, b| a.path.cmp(&b.path));
    uncommitted.dedup_by(|a, b| a.path == b.path);
    let check = check_after_last_change(persist, caller.session_id, &caller.checkout).await?;
    let evidence = evidence_on(persist, caller, &uncommitted).await?;
    Ok(MineAnswer {
        uncommitted,
        check,
        evidence,
    })
}

async fn evidence_on(
    persist: &PersistHandle,
    caller: &Caller,
    uncommitted: &[MinePath],
) -> Result<Vec<ChangeLine>, AnswerError> {
    if uncommitted.is_empty() {
        return Ok(Vec::new());
    }
    let prefixes: Vec<String> = uncommitted.iter().map(|p| p.path.clone()).collect();
    let events = persist
        .fleet()
        .latest_change_events(
            &caller.checkout,
            &prefixes,
            &[ChangeKind::Check, ChangeKind::Verdict],
            EVIDENCE_EVENTS,
        )
        .await?;
    let names = names_for(persist, &events).await?;
    Ok(events.iter().map(|e| line_of(e, &names)).collect())
}

pub async fn check_after_last_change(
    persist: &PersistHandle,
    session_id: i64,
    checkout: &str,
) -> Result<CheckAfterLastChange, AnswerError> {
    let changes = persist
        .fleet()
        .latest_change_events(checkout, &[], &CHANGE_KINDS, RECENT_EVENTS)
        .await?;
    let last_change = changes
        .into_iter()
        .find(|e| e.session_id == Some(session_id));
    let checks = persist
        .fleet()
        .latest_change_events(checkout, &[], &[ChangeKind::Check], CHECK_EVENTS)
        .await?;
    Ok(judge(last_change.as_ref(), checks.first()))
}

fn judge(
    last_change: Option<&ChangeEvent>,
    newest_check: Option<&ChangeEvent>,
) -> CheckAfterLastChange {
    let after = match (last_change, newest_check) {
        (Some(change), Some(check)) => check.id > change.id,
        (None, Some(_)) => true,
        (_, None) => false,
    };
    let verdict = newest_check
        .filter(|_| after)
        .and_then(|check| {
            check
                .detail
                .get("passed")
                .and_then(serde_json::Value::as_bool)
        })
        .map(|passed| {
            if passed {
                CheckVerdict::Passed
            } else {
                CheckVerdict::Failed
            }
        });
    CheckAfterLastChange {
        ran: after,
        verdict,
        last_change_at: last_change.map(|c| c.at),
        checked_at: newest_check.filter(|_| after).map(|c| c.at),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::store::persistence::change_record::test_seed::{self, CHECKOUT};
    use crate::store::persistence::test_support;

    fn event(id: i64, kind: ChangeKind, passed: Option<bool>) -> ChangeEvent {
        ChangeEvent {
            id,
            project_id: 1,
            checkout: CHECKOUT.to_owned(),
            at: Utc::now(),
            kind,
            session_id: Some(1),
            run_id: None,
            task: None,
            paths: Vec::new(),
            subject: String::new(),
            detail: passed.map_or_else(
                || serde_json::json!({}),
                |p| serde_json::json!({"passed": p}),
            ),
            before_sha: None,
            after_sha: None,
        }
    }

    #[test]
    fn a_check_newer_than_the_last_change_ran_after_it_and_carries_its_verdict() {
        let change = event(5, ChangeKind::Edit, None);
        let check = event(9, ChangeKind::Check, Some(true));
        let judged = judge(Some(&change), Some(&check));
        assert!(judged.ran);
        assert_eq!(judged.verdict, Some(CheckVerdict::Passed));
        assert!(judged.checked_at.is_some());
    }

    #[test]
    fn a_check_older_than_the_last_change_did_not_run_after_it() {
        let change = event(9, ChangeKind::Edit, None);
        let check = event(5, ChangeKind::Check, Some(false));
        let judged = judge(Some(&change), Some(&check));
        assert!(!judged.ran);
        assert_eq!(judged.verdict, None);
        assert_eq!(judged.checked_at, None);
    }

    #[test]
    fn with_no_check_recorded_nothing_ran() {
        let change = event(1, ChangeKind::Edit, None);
        assert!(!judge(Some(&change), None).ran);
        assert!(!judge(None, None).ran);
    }

    #[tokio::test]
    async fn mine_lists_my_open_paths_and_the_check_that_followed_them() -> anyhow::Result<()> {
        let Some(db) = test_support::acquire_ephemeral_db().await? else {
            eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        let project = test_seed::project(handle.pool()).await?;
        let me = test_seed::session(handle.pool(), project, None).await?;
        let other = test_seed::session(handle.pool(), project, None).await?;
        let mut mine_edit = test_seed::event(project, ChangeKind::Edit, &["src/a.rs"]);
        mine_edit.session_id = Some(me);
        let mut theirs = test_seed::event(project, ChangeKind::Edit, &["src/b.rs"]);
        theirs.session_id = Some(other);
        let mut check = test_seed::event(project, ChangeKind::Check, &["src/a.rs"]);
        check.detail = serde_json::json!({"passed": false});
        handle
            .fleet()
            .insert_change_events(&[mine_edit, theirs])
            .await?;
        let caller = Caller {
            session_id: me,
            checkout: CHECKOUT.to_owned(),
            in_fleet: false,
            task_files: Vec::new(),
        };
        let before = mine(&handle, &caller).await?;
        let paths: Vec<&str> = before.uncommitted.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, vec!["src/a.rs"]);
        assert!(!before.check.ran, "{before:?}");
        handle.fleet().insert_change_events(&[check]).await?;
        let after = mine(&handle, &caller).await?;
        assert!(after.check.ran, "{after:?}");
        assert_eq!(after.check.verdict, Some(CheckVerdict::Failed));
        assert_eq!(after.evidence.len(), 1, "{after:?}");
        let empty = mine(
            &handle,
            &Caller {
                session_id: other + 1000,
                ..caller
            },
        )
        .await?;
        assert!(empty.uncommitted.is_empty() && empty.evidence.is_empty());
        db.cleanup().await?;
        Ok(())
    }
}
