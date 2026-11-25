use super::fakes_tests::{CHECK, Env, FakeRunner, FakeSessions, REPO, ok, remote_fixer};
use super::landed::{close_landed, landed_followups};
use super::runner::Program;
use crate::store::persistence::wright::FollowupLimitsPatch;

#[test]
fn landed_followups_reads_every_trailer_of_a_commit() {
    let raw = "\u{1e}bbb\u{1f}two\n\nWright-Followup: 7\nWright-Followup: 9\n\u{1e}aaa\u{1f}one\n\nWright-Followup: 7\nWright-Followup: x";
    let landed = landed_followups(raw);
    assert_eq!(landed.get(&7).map(String::as_str), Some("bbb"));
    assert_eq!(landed.get(&9).map(String::as_str), Some("bbb"));
    assert_eq!(landed.len(), 2);
}

#[tokio::test]
async fn close_landed_settles_filed_and_open_followups_named_on_main() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let limits = env
        .limits(FollowupLimitsPatch {
            fixers: Some(vec![remote_fixer()]),
            fix_host: Some("infra".to_owned()),
            ..FollowupLimitsPatch::default()
        })
        .await?;
    let filed = env.filed("launch", 61).await?;
    let (open, _) = env.followup("hardening", "mailbox").await?;
    let (other, _) = env.followup("hardening", "elsewhere").await?;
    let log = format!("\u{1e}abc\u{1f}fix\n\nWright-Followup: {filed}\nWright-Followup: {open}");
    let runner = FakeRunner::new()
        .on(Program::Ssh, "origin/main", ok(&log))
        .on(Program::Gh, "issue close 61", ok(""));
    let ctx = env.ctx(runner, FakeSessions { ended: None });
    assert_eq!(close_landed(&ctx, &limits).await?, Vec::new());
    assert_eq!(env.state_of(filed).await?.0, "fixed");
    assert_eq!(env.state_of(open).await?.0, "dropped");
    assert_eq!(env.state_of(other).await?.0, "open");
    let closed = ctx.runner.called(Program::Gh, "issue close");
    assert_eq!(closed.len(), 1, "{CHECK} in {REPO}");
    env.db.cleanup().await
}
