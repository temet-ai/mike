use async_trait::async_trait;
use chrono::{DateTime, Duration, TimeZone, Utc};
use tokio::sync::Mutex;

use super::bar_evidence::LabelRow;
use super::bar_gh::IssueTracker;
use super::bar_issues::{Action, BarIssue, Context, file, title};
use super::{PointScore, score_point};
use crate::llm::decisions::loop_store::{LabelledDecision, PointCounts};

pub(crate) struct Fake {
    open: Option<u32>,
    milestone: bool,
    fail: bool,
    calls: Mutex<Vec<String>>,
    bodies: Mutex<Vec<String>>,
}

impl Fake {
    pub(crate) fn new(open: Option<u32>, milestone: bool) -> Self {
        Self {
            open,
            milestone,
            fail: false,
            calls: Mutex::new(Vec::new()),
            bodies: Mutex::new(Vec::new()),
        }
    }

    fn failing() -> Self {
        Self {
            fail: true,
            ..Self::new(None, true)
        }
    }

    pub(crate) async fn calls(&self) -> Vec<String> {
        self.calls.lock().await.clone()
    }

    async fn body(&self) -> String {
        self.bodies.lock().await.join("\n")
    }

    async fn note(&self, call: String) {
        self.calls.lock().await.push(call);
    }
}

#[async_trait]
impl IssueTracker for Fake {
    async fn open_issue(&self, title: &str) -> Result<Option<u32>, String> {
        self.note(format!("open:{title}")).await;
        if self.fail {
            return Err("gh is missing".to_owned());
        }
        Ok(self.open)
    }

    async fn milestone_open(&self, name: &str) -> Result<bool, String> {
        self.note(format!("milestone:{name}")).await;
        Ok(self.milestone)
    }

    async fn create(
        &self,
        title: &str,
        body: &str,
        milestone: Option<&str>,
    ) -> Result<u32, String> {
        self.note(format!("create:{title}:{milestone:?}")).await;
        self.bodies.lock().await.push(body.to_owned());
        Ok(501)
    }

    async fn comment(&self, issue: u32, body: &str) -> Result<(), String> {
        self.note(format!("comment:{issue}")).await;
        self.bodies.lock().await.push(body.to_owned());
        Ok(())
    }
}

pub(crate) fn scored(point: &str, right: usize, wrong: usize) -> PointScore {
    let labelled: Vec<LabelledDecision> = (0..right + wrong)
        .map(|i| LabelledDecision {
            point: point.to_owned(),
            score: Some(0.5),
            applied: Some("sent".to_owned()),
            label: if i < right { "right" } else { "wrong" }.to_owned(),
        })
        .collect();
    let counts = PointCounts {
        point: point.to_owned(),
        calls: 100,
        applied_on: 90,
        labelled: labelled.len() as i64,
        ..PointCounts::default()
    };
    score_point(counts, &labelled)
}

fn rows(point: &str, right: usize, wrong: usize) -> Vec<LabelRow> {
    (1..=(right + wrong) as i64)
        .map(|id| LabelRow {
            id,
            point: point.to_owned(),
            label: if id as usize <= right {
                "right"
            } else {
                "wrong"
            }
            .to_owned(),
            label_source: if id % 2 == 0 { "guard" } else { "operator" }.to_owned(),
        })
        .collect()
}

fn context() -> Context {
    Context {
        run: 42,
        now: Utc
            .with_ymd_and_hms(2026, 11, 15, 6, 0, 0)
            .single()
            .unwrap_or_default(),
    }
}

fn earlier(ctx: &Context, hours: i64) -> DateTime<Utc> {
    ctx.now - Duration::hours(hours)
}

#[test]
fn a_point_is_under_the_bar_only_with_twenty_labels_and_less_than_three_quarters_right() {
    assert!(!scored("p", 10, 9).under_bar());
    assert!(!scored("p", 15, 5).under_bar());
    assert!(scored("p", 14, 6).under_bar());
    assert!(scored("p", 0, 20).under_bar());
    let score = scored("p", 12, 12);
    assert_eq!((score.labelled, score.right), (24, 12));
    assert_eq!(score.share, Some(0.5));
    assert!(
        score
            .findings
            .iter()
            .any(|f| f.contains("under the bar 75%"))
    );
}

#[tokio::test]
async fn with_no_open_issue_one_is_created_with_the_milestone_and_the_figures() {
    let tracker = Fake::new(None, true);
    let score = scored("turn_guard", 12, 12);
    let filed = file(
        &tracker,
        &[&score],
        &rows("turn_guard", 12, 12),
        &[],
        &context(),
    )
    .await;
    let want = title("turn_guard");
    assert_eq!(want, "Jev decision point turn_guard under 75% right");
    assert_eq!(
        tracker.calls().await,
        vec![
            format!("open:{want}"),
            "milestone:Jev decisions".to_owned(),
            format!("create:{want}:Some(\"Jev decisions\")"),
        ]
    );
    assert_eq!(
        filed,
        vec![BarIssue {
            point: "turn_guard".to_owned(),
            issue: 501,
            action: Action::Created,
            at: context().now,
        }]
    );
    let body = tracker.body().await;
    for needle in [
        "right on 12 of 24 labelled outcomes (50%)",
        "- guard: 6/12 right (50%)",
        "- operator: 6/12 right (50%)",
        "24, 23, 22, 21, 20",
        "run 42",
        "14 days",
    ] {
        assert!(body.contains(needle), "{needle} missing from {body}");
    }
}

#[tokio::test]
async fn without_the_milestone_the_issue_is_created_without_one() {
    let tracker = Fake::new(None, false);
    let score = scored("p", 5, 20);
    file(&tracker, &[&score], &rows("p", 5, 20), &[], &context()).await;
    let calls = tracker.calls().await;
    assert_eq!(
        calls.last().map(String::as_str),
        Some("create:Jev decision point p under 75% right:None")
    );
}

#[tokio::test]
async fn with_an_open_issue_a_comment_is_added_and_nothing_is_created() {
    let tracker = Fake::new(Some(77), true);
    let score = scored("p", 12, 12);
    let filed = file(&tracker, &[&score], &rows("p", 12, 12), &[], &context()).await;
    assert_eq!(
        tracker.calls().await,
        vec![format!("open:{}", title("p")), "comment:77".to_owned()]
    );
    assert_eq!(filed.len(), 1);
    assert_eq!(filed[0].action, Action::Commented);
    assert_eq!(filed[0].issue, 77);
    assert!(
        tracker
            .body()
            .await
            .starts_with("Update from decisions loop run 42")
    );
}

#[tokio::test]
async fn a_point_commented_less_than_a_day_ago_is_left_alone() {
    let tracker = Fake::new(Some(77), true);
    let ctx = context();
    let score = scored("p", 12, 12);
    let recent = BarIssue {
        point: "p".to_owned(),
        issue: 77,
        action: Action::Commented,
        at: earlier(&ctx, 23),
    };
    let filed = file(&tracker, &[&score], &rows("p", 12, 12), &[recent], &ctx).await;
    assert!(filed.is_empty());
    assert!(tracker.calls().await.is_empty());
}

#[tokio::test]
async fn a_comment_older_than_a_day_or_for_another_point_does_not_block() {
    let tracker = Fake::new(Some(77), true);
    let ctx = context();
    let score = scored("p", 12, 12);
    let previous = [
        BarIssue {
            point: "p".to_owned(),
            issue: 77,
            action: Action::Commented,
            at: earlier(&ctx, 25),
        },
        BarIssue {
            point: "other".to_owned(),
            issue: 80,
            action: Action::Created,
            at: earlier(&ctx, 1),
        },
    ];
    let filed = file(&tracker, &[&score], &rows("p", 12, 12), &previous, &ctx).await;
    assert_eq!(filed.len(), 1);
    assert_eq!(
        tracker.calls().await.last().map(String::as_str),
        Some("comment:77")
    );
}

#[tokio::test]
async fn a_tracker_that_fails_files_nothing_and_does_not_stop_the_other_points() {
    let tracker = Fake::failing();
    let score = scored("p", 12, 12);
    let filed = file(&tracker, &[&score], &rows("p", 12, 12), &[], &context()).await;
    assert!(filed.is_empty());
    assert_eq!(tracker.calls().await, vec![format!("open:{}", title("p"))]);
}
