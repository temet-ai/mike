use crate::store::persistence::web_repo::WebPersist as _;
use chrono::{Duration, Utc};
use uuid::Uuid;

use super::context::{Machine, Program};
use super::reconcile::{GRACE, Liveness, Verdict, judge, reconcile};
use super::test_support::{HOST, Rig};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::wright::{Inserted, LiveRun, NewInvestigation};

fn me() -> Machine {
    Machine {
        host: HOST.to_owned(),
        uuid: None,
    }
}

fn row(state: &str, host: &str, pid: Option<i32>, deadline_in: Duration) -> LiveRun {
    LiveRun {
        id: 1,
        state: state.to_owned(),
        host: host.to_owned(),
        machine_uuid: None,
        pid,
        deadline_at: Utc::now() + deadline_in,
    }
}

fn yes(_: i32) -> bool {
    true
}

fn no(_: i32) -> bool {
    false
}

const ALIVE: Liveness = Liveness {
    starter: yes,
    sandbox: yes,
};
const DEAD: Liveness = Liveness {
    starter: no,
    sandbox: no,
};

#[test]
fn investigate_judge_reconciles_dead_or_uncertain_rows_of_this_machine() {
    let now = Utc::now();
    let soon = Duration::minutes(10);
    assert_eq!(
        judge(&row("starting", HOST, Some(9), soon), &me(), now, &ALIVE),
        Verdict::Keep
    );
    assert!(
        matches!(judge(&row("starting", HOST, Some(9), soon), &me(), now, &DEAD), Verdict::Reconcile(r) if r.contains("launch uncertain"))
    );
    assert!(matches!(
        judge(&row("starting", HOST, None, soon), &me(), now, &ALIVE),
        Verdict::Reconcile(_)
    ));
    assert!(
        matches!(judge(&row("running", HOST, Some(9), soon), &me(), now, &DEAD), Verdict::Reconcile(r) if r.contains("gone"))
    );
    let overdue = -(GRACE + Duration::minutes(1));
    assert!(matches!(
        judge(&row("running", HOST, Some(9), overdue), &me(), now, &ALIVE),
        Verdict::KillThenReconcile(9, r) if r.contains("outlived")
    ));
    assert!(matches!(
        judge(&row("starting", HOST, Some(9), overdue), &me(), now, &ALIVE),
        Verdict::Reconcile(_)
    ));
}

#[test]
fn investigate_judge_waits_for_another_machine_until_deadline_and_grace() {
    let now = Utc::now();
    assert_eq!(
        judge(
            &row("running", "other-host", Some(9), Duration::minutes(10)),
            &me(),
            now,
            &DEAD
        ),
        Verdict::Keep
    );
    let overdue = -(GRACE + Duration::minutes(1));
    assert!(matches!(
        judge(&row("running", "other-host", Some(9), overdue), &me(), now, &DEAD),
        Verdict::Reconcile(r) if r.contains("other-host")
    ));
}

async fn insert(persist: &PersistHandle, host: &str, pid: i32) -> anyhow::Result<Inserted> {
    Ok(persist
        .web()
        .insert_wright_run(&NewInvestigation {
            mode: "investigate",
            problem_id: None,
            session_uuid: Uuid::new_v4(),
            host,
            machine_uuid: None,
            owner_pid: pid,
        })
        .await?)
}

fn dead_pid() -> anyhow::Result<i32> {
    let mut child = std::process::Command::new("/usr/bin/true").spawn()?;
    let pid = i32::try_from(child.id())?;
    child.wait()?;
    Ok(pid)
}

async fn state_of(persist: &PersistHandle, id: i64) -> anyhow::Result<(String, Option<String>)> {
    let run = persist
        .web()
        .wright_run(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("run {id} vanished"))?;
    Ok((run.state, run.end_reason))
}

#[tokio::test]
async fn investigate_stale_starting_row_is_reconciled_before_a_retry() -> anyhow::Result<()> {
    let rig = Rig::boot(Program::Claude).await?;
    let persist = &rig.ctx.persist;
    let Inserted::Started { id: stale, .. } = insert(persist, HOST, dead_pid()?).await? else {
        anyhow::bail!("the first start was refused");
    };
    assert_eq!(insert(persist, HOST, 1).await?, Inserted::Busy);
    let done = reconcile(persist, &me()).await?;
    assert_eq!(done, vec![stale]);
    let (state, reason) = state_of(persist, stale).await?;
    assert_eq!(state, "reconciled");
    assert!(reason.is_some_and(|r| r.contains("launch uncertain")));
    assert!(matches!(
        insert(persist, HOST, 1).await?,
        Inserted::Started { .. }
    ));
    rig.cleanup().await
}

#[tokio::test]
async fn investigate_live_row_of_another_machine_blocks_a_start() -> anyhow::Result<()> {
    let rig = Rig::boot(Program::Claude).await?;
    let persist = &rig.ctx.persist;
    let Inserted::Started { id, .. } = insert(persist, "other-host", 4242).await? else {
        anyhow::bail!("the first start was refused");
    };
    assert!(reconcile(persist, &me()).await?.is_empty());
    assert_eq!(insert(persist, HOST, 1).await?, Inserted::Busy);
    sqlx::query(
        "UPDATE wright.investigations SET deadline_at = now() - interval '1 hour' WHERE id = $1",
    )
    .bind(id)
    .execute(persist.pool())
    .await?;
    assert_eq!(reconcile(persist, &me()).await?, vec![id]);
    assert!(matches!(
        insert(persist, HOST, 1).await?,
        Inserted::Started { .. }
    ));
    rig.cleanup().await
}
