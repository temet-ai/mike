use uuid::Uuid;

use super::fakes_tests::{
    BUSY_PROBE, Env, FakeRunner, FakeSessions, QUIET_PROBE, ok, remote_fixer, spawned,
};
use super::handoff::{Handoff, hand_off};
use super::runner::Program;
use crate::store::persistence::wright::FollowupLimitsPatch;

fn sessions() -> FakeSessions {
    FakeSessions { ended: None }
}

fn remote() -> FollowupLimitsPatch {
    FollowupLimitsPatch {
        fixers: Some(vec![remote_fixer()]),
        ..FollowupLimitsPatch::default()
    }
}

#[tokio::test]
async fn handoff_with_a_live_fixer_starts_no_other() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let limits = env.limits(remote()).await?;
    env.handed("launch", 50).await?;
    let waiting = env.filed("mailbox", 51).await?;
    let runner = FakeRunner::new().on(Program::Ssh, "loadavg", ok(QUIET_PROBE));
    let ctx = env.ctx(runner, sessions());
    assert_eq!(hand_off(&ctx, &limits).await?, Handoff::FixerLive(1));
    assert!(ctx.runner.calls().is_empty());
    assert_eq!(env.state_of(waiting).await?.0, "filed");
    env.db.cleanup().await
}

#[tokio::test]
async fn handoff_with_no_quiet_machine_starts_nothing() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let limits = env.limits(remote()).await?;
    let waiting = env.filed("launch", 52).await?;
    let runner = FakeRunner::new().on(Program::Ssh, "loadavg", ok(BUSY_PROBE));
    let ctx = env.ctx(runner, sessions());
    let Handoff::NoQuietMachine(reasons) = hand_off(&ctx, &limits).await? else {
        anyhow::bail!("expected no quiet machine");
    };
    assert!(reasons[0].contains("12.50"), "{reasons:?}");
    assert!(ctx.runner.called(Program::Ssh, "spawn").is_empty());
    assert_eq!(env.state_of(waiting).await?.0, "filed");
    env.db.cleanup().await
}

#[tokio::test]
async fn handoff_quiet_remote_machine_spawns_fixer_and_records_it() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let limits = env.limits(remote()).await?;
    let id = env.filed("launch", 53).await?;
    let session = Uuid::new_v4();
    let runner = FakeRunner::new()
        .on(Program::Ssh, "loadavg", ok(QUIET_PROBE))
        .on(Program::Ssh, "spawn", ok(&spawned(session)));
    let ctx = env.ctx(runner, sessions());
    assert_eq!(
        hand_off(&ctx, &limits).await?,
        Handoff::HandedOff {
            id,
            session,
            machine: "infra".to_owned()
        }
    );
    let spawns = ctx.runner.called(Program::Ssh, "spawn");
    let [spawn] = spawns.as_slice() else {
        anyhow::bail!("expected one spawn, got {spawns:?}");
    };
    assert!(spawn.has_args(&["--", "infra"]));
    let line = spawn.args.last().cloned().unwrap_or_default();
    assert!(
        line.starts_with("mike 'spawn' 'open' '/srv/mike' '--json'"),
        "{line}"
    );
    assert!(line.contains("'--goal' 'Fix temet-ai/mike#53'"), "{line}");
    assert!(
        line.contains(&format!("'--session-name' 'wright-fix-{id}'")),
        "{line}"
    );
    let prompt = spawn.stdin.clone().unwrap_or_default();
    assert!(prompt.starts_with("You are Wright's fixer for temet-ai/mike#53."));
    let row = sqlx::query_as::<_, (String, Option<Uuid>, Option<String>)>(
        "SELECT state, fixer_session, fixer_machine FROM wright.followups WHERE id = $1",
    )
    .bind(id)
    .fetch_one(env.persist.pool())
    .await?;
    assert_eq!(
        row,
        (
            "handed_off".to_owned(),
            Some(session),
            Some("infra".to_owned())
        )
    );
    env.db.cleanup().await
}
