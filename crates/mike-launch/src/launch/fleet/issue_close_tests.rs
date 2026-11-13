#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use std::sync::Mutex;

use async_trait::async_trait;
use uuid::Uuid;

use super::issue_close::{Recorded, Tracker, due, passed, settle};
use super::status::{TaskStatus, Verdict};

fn task(id: &str, passed_all: bool, verdict: &str) -> TaskStatus {
    TaskStatus {
        id: id.into(),
        name: "the task".into(),
        discipline: "backend".into(),
        criteria: vec!["C1".into()],
        seat: None,
        last_seat: None,
        state: "working".into(),
        passes: 0,
        fails: 0,
        rounds: 0,
        last: Some(Verdict {
            at: chrono::Utc::now(),
            verdict: verdict.into(),
            title: "t".into(),
            tasks: vec![id.into()],
            criteria: vec!["C1".into()],
        }),
        passed_criteria: if passed_all {
            vec!["C1".into()]
        } else {
            vec![]
        },
        ready: false,
    }
}

fn record(task: &str, issues: &[u32], closed: bool) -> Recorded {
    Recorded {
        note: Uuid::from_u128(1),
        task: task.into(),
        sha: "0123456789abcdef".into(),
        issues: issues.to_vec(),
        closed,
    }
}

#[derive(Default)]
struct Fake {
    open: Vec<u32>,
    closed: Mutex<Vec<(u32, String)>>,
}

#[async_trait]
impl Tracker for Fake {
    async fn is_open(&self, issue: u32) -> Result<bool, String> {
        Ok(self.open.contains(&issue))
    }

    async fn close(&self, issue: u32, comment: &str) -> Result<(), String> {
        self.closed.lock().unwrap().push((issue, comment.into()));
        Ok(())
    }
}

#[test]
fn a_task_acceptance_passed_closes_the_issues_its_commits_refer_to() {
    let tasks = [task("T2", true, "pass")];
    let closes = due(&tasks, &[record("T2", &[531, 12], false)]);
    assert_eq!(
        closes.iter().map(|c| c.issue).collect::<Vec<_>>(),
        [531, 12]
    );
    assert!(closes[0].comment.contains("T2"));
    assert!(closes[0].comment.contains("0123456789"));
}

#[test]
fn a_failed_or_unfinished_task_leaves_its_issues_open() {
    let refs = [record("T2", &[531], false)];
    assert!(due(&[task("T2", false, "fail")], &refs).is_empty());
    assert!(due(&[task("T2", true, "fail")], &refs).is_empty());
    assert!(!passed(&task("T2", false, "pass")));
    assert!(due(&[task("T9", true, "pass")], &refs).is_empty());
}

#[test]
fn issues_already_closed_by_mike_are_not_closed_again() {
    let tasks = [task("T2", true, "pass")];
    assert!(due(&tasks, &[record("T2", &[531], true)]).is_empty());
}

#[tokio::test]
async fn settle_closes_open_issues_and_skips_ones_already_closed() {
    let tasks = [task("T2", true, "pass")];
    let closes = due(&tasks, &[record("T2", &[531, 12], false)]);
    let tracker = Fake {
        open: vec![531],
        ..Fake::default()
    };
    let done = settle(&tracker, &closes).await;
    assert_eq!(
        done.iter().map(|(_, r)| r.clone()).collect::<Vec<_>>(),
        [Ok(true), Ok(false)]
    );
    let closed = tracker.closed.lock().unwrap();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].0, 531);
}
