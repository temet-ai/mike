use super::guard::Guard;
use super::llm::Failure;
use super::test_support::{covered_answer, covered_lesson_scenario, harness};
use super::worker::{Outcome, tick};
use crate::store::persistence::PersistHandle;
use uuid::Uuid;

const BODY: &str = "- Keep this one.\n- Commits use jj.\n";

#[tokio::test]
async fn nothing_due_is_idle_and_opens_no_run() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let outcome = tick(&h.fx.handle, &h.llm, &h.emb, &h.settings, &mut h.guard, None).await;
    assert_eq!(outcome, Outcome::Idle);
    let runs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.consolidation_runs r
           JOIN memory.consolidation_run_kinds k ON k.id = r.run_kind_id WHERE k.code = 'knowledge'",
    )
    .fetch_one(h.fx.handle.pool())
    .await?;
    assert_eq!(runs, 0);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_pass_with_work_records_a_finished_run_decisions_and_a_measure() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let s = covered_lesson_scenario(&mut h, BODY, "Commits use jj.").await?;
    h.llm.push_judge(Ok(covered_answer(s.lesson_statement, s.file_statement)?));
    h.fx.make_subjects_due().await?;

    let outcome = tick(&h.fx.handle, &h.llm, &h.emb, &h.settings, &mut h.guard, None).await;
    let Outcome::Ran(tally) = outcome else {
        anyhow::bail!("expected a pass, got {outcome:?}");
    };
    assert_eq!(tally.subjects_judged, 1);
    let (status, finished, decisions): (String, bool, i64) = sqlx::query_as(
        "SELECT s.code, r.finished_at IS NOT NULL,
                (SELECT count(*) FROM memory.knowledge_consolidation_decisions d WHERE d.run_id = r.id)
           FROM memory.consolidation_runs r
           JOIN memory.consolidation_run_kinds k ON k.id = r.run_kind_id AND k.code = 'knowledge'
           JOIN memory.consolidation_run_statuses s ON s.id = r.status_id",
    )
    .fetch_one(h.fx.handle.pool())
    .await?;
    assert_eq!((status.as_str(), finished, decisions), ("completed", true, 2));
    let (statements, distance): (i32, f32) = sqlx::query_as(
        "SELECT statements_current, grouping_distance FROM memory.knowledge_consolidation_measures",
    )
    .fetch_one(h.fx.handle.pool())
    .await?;
    assert_eq!(statements, 1);
    assert!((distance - 0.70).abs() < 1e-6);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_judge_that_ends_in_many_misgrouped_members_raises_the_projects_similarity() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let s = covered_lesson_scenario(&mut h, BODY, "Commits use jj.").await?;
    h.llm.push_judge(Ok(super::test_support::judge(serde_json::json!({
        "subject": "x", "text": "x",
        "members": [
            {"id": s.lesson_statement, "role": "elsewhere", "by": null, "note": "", "evidence_for": null},
            {"id": s.file_statement, "role": "keep", "by": null, "note": "", "evidence_for": null}],
        "pull_in": [], "evidence_used": [], "merges": []
    }))?));
    h.fx.make_subjects_due().await?;
    tick(&h.fx.handle, &h.llm, &h.emb, &h.settings, &mut h.guard, None).await;
    let (distance, misgrouped): (f32, f32) = sqlx::query_as(
        "SELECT grouping_distance, misgrouped_share FROM memory.knowledge_consolidation_measures
          ORDER BY id DESC LIMIT 1",
    )
    .fetch_one(h.fx.handle.pool())
    .await?;
    assert!((distance - 0.71).abs() < 1e-6, "{distance}");
    assert!(misgrouped > 0.0);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_machine_that_does_not_hold_the_lease_does_no_work() -> anyhow::Result<()> {
    let Some(h) = harness(1).await? else {
        return Ok(());
    };
    let project = h.fx.project().await?;
    h.fx.lesson(project, "Use jj.").await?;
    let leader = PersistHandle::connect(&h.fx.db.database_url).await?.with_llm_machine(Uuid::new_v4());
    let follower = PersistHandle::connect(&h.fx.db.database_url).await?.with_llm_machine(Uuid::new_v4());
    let first = tick(&leader, &h.llm, &h.emb, &h.settings, &mut Guard::new(), None).await;
    assert!(matches!(first, Outcome::Ran(_)));
    let second = tick(&follower, &h.llm, &h.emb, &h.settings, &mut Guard::new(), None).await;
    assert_eq!(second, Outcome::NotLeader);
    h.fx.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn an_open_circuit_and_a_disabled_embedder_skip_the_pass_and_failures_are_reported() -> anyhow::Result<()> {
    let Some(mut h) = harness(1).await? else {
        return Ok(());
    };
    let project = h.fx.project().await?;
    h.fx.lesson(project, "Use jj.").await?;
    for _ in 0..3 {
        h.guard.failed(&Failure::transient("down"), std::time::Instant::now());
    }
    let open = tick(&h.fx.handle, &h.llm, &h.emb, &h.settings, &mut h.guard, None).await;
    assert_eq!(open, Outcome::Skipped("circuit open"));

    h.emb.available = false;
    let disabled = tick(&h.fx.handle, &h.llm, &h.emb, &h.settings, &mut Guard::new(), None).await;
    assert_eq!(disabled, Outcome::Skipped("embedding disabled"));
    h.emb.available = true;
    h.emb.fail_with(Some(Failure::transient("embedding down")));
    let Outcome::Ran(tally) = tick(&h.fx.handle, &h.llm, &h.emb, &h.settings, &mut Guard::new(), None).await else {
        anyhow::bail!("expected a pass");
    };
    assert_eq!(tally.items_deferred, 1);
    let status: String = sqlx::query_scalar(
        "SELECT s.code FROM memory.consolidation_runs r
           JOIN memory.consolidation_run_statuses s ON s.id = r.status_id ORDER BY r.id DESC LIMIT 1",
    )
    .fetch_one(h.fx.handle.pool())
    .await?;
    assert_eq!(status, "partial");
    h.fx.db.cleanup().await?;
    Ok(())
}
