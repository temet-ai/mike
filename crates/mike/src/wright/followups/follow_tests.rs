use crate::store::persistence::web_repo::WebPersist as _;
use uuid::Uuid;

use super::fakes_tests::{Env, FakeRunner, FakeSessions, ok};
use super::follow::{Followed, follow};
use super::runner::Program;
use crate::store::persistence::wright::FollowupLimitsPatch;

const OPEN: &str = r#"{"closedByPullRequestsReferences":[],"state":"OPEN","stateReason":""}"#;

fn closed_by_pr() -> String {
    serde_json::json!({
        "closedByPullRequestsReferences": [{"number": 301, "url": "https://github.com/temet-ai/mike/pull/301"}],
        "state": "CLOSED",
        "stateReason": "COMPLETED"
    })
    .to_string()
}

#[tokio::test]
async fn follow_closed_issue_marks_followup_fixed() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let limits = env.limits(FollowupLimitsPatch::default()).await?;
    let id = env.handed("launch", 60).await?;
    let runner = FakeRunner::new().on(Program::Gh, "issue view 60", ok(&closed_by_pr()));
    let ctx = env.ctx(runner, FakeSessions { ended: None });
    let report = follow(&ctx, &limits).await?;
    let [(followed_id, Followed::Fixed(how))] = report.followed.as_slice() else {
        anyhow::bail!("expected one fixed follow-up, got {report:?}");
    };
    assert_eq!(*followed_id, id);
    assert!(how.contains("#301"), "{how}");
    assert_eq!(env.state_of(id).await?.0, "fixed");
    env.db.cleanup().await
}

#[tokio::test]
async fn follow_issue_closed_not_planned_marks_followup_dropped() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let limits = env.limits(FollowupLimitsPatch::default()).await?;
    let id = env.handed("launch", 61).await?;
    let view =
        r#"{"closedByPullRequestsReferences":[],"state":"CLOSED","stateReason":"NOT_PLANNED"}"#;
    let runner = FakeRunner::new().on(Program::Gh, "issue view 61", ok(view));
    let ctx = env.ctx(runner, FakeSessions { ended: None });
    follow(&ctx, &limits).await?;
    assert_eq!(env.state_of(id).await?.0, "dropped");
    env.db.cleanup().await
}

#[tokio::test]
async fn follow_open_issue_with_live_fixer_leaves_it_handed_off() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let limits = env.limits(FollowupLimitsPatch::default()).await?;
    let id = env.handed("launch", 62).await?;
    let runner = FakeRunner::new().on(Program::Gh, "issue view 62", ok(OPEN));
    let ctx = env.ctx(runner, FakeSessions { ended: None });
    let report = follow(&ctx, &limits).await?;
    assert_eq!(report.followed, vec![(id, Followed::Working)]);
    assert_eq!(env.state_of(id).await?.0, "handed_off");
    env.db.cleanup().await
}

#[tokio::test]
async fn follow_ended_fixer_returns_followup_then_stuck_after_max_attempts() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let limits = env
        .limits(FollowupLimitsPatch {
            max_attempts: Some(2),
            ..FollowupLimitsPatch::default()
        })
        .await?;
    let id = env.handed("launch", 63).await?;
    let runner = FakeRunner::new().on(Program::Gh, "issue view 63", ok(OPEN));
    let ended = Some("its launch closed".to_owned());
    let ctx = env.ctx(runner, FakeSessions { ended });
    follow(&ctx, &limits).await?;
    assert_eq!(env.state_of(id).await?, ("filed".to_owned(), 1, Some(63)));
    anyhow::ensure!(
        env.persist
            .web()
            .mark_wright_followup_handed_off(id, Uuid::new_v4(), "infra")
            .await?
    );
    let report = follow(&ctx, &limits).await?;
    let [(_, Followed::Returned { state, why })] = report.followed.as_slice() else {
        anyhow::bail!("expected one returned follow-up, got {report:?}");
    };
    assert_eq!(state, "stuck");
    assert_eq!(why, "its launch closed");
    assert_eq!(env.state_of(id).await?, ("stuck".to_owned(), 2, Some(63)));
    env.db.cleanup().await
}
