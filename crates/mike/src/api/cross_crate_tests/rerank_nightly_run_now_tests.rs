use crate::api::cross_crate_tests::rerank_nightly_worker_tests::{
    graded_count, seed_events, seeded_documents, tuning_on,
};
use crate::api::rerank_search::harness::Harness;
pub(crate) use crate::rerank_nightly::run_now::{RunNow, run_pass_now};

#[tokio::test]
async fn run_now_with_tuning_off_is_refused_and_grades_nothing() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness.enable_rerank(2_000);
    harness.reranker_prefers_later_documents(None).await;
    let (project, references) = seeded_documents(&harness).await?;
    let (_, events) = seed_events(&harness, project, &[&references[0]], 2).await?;

    let outcome = run_pass_now(&harness.state).await;

    assert_eq!(outcome, RunNow::TuningOff);
    assert_eq!(graded_count(&harness, &events).await?, 0);
    harness.cleanup().await
}

#[tokio::test]
async fn run_now_grades_every_waiting_search_without_waiting_for_the_quiet_window()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness.enable_rerank(2_000);
    harness.reranker_prefers_later_documents(None).await;
    tuning_on(&harness);
    let (project, references) = seeded_documents(&harness).await?;
    let (_, events) = seed_events(&harness, project, &[&references[0]], 3).await?;

    let outcome = run_pass_now(&harness.state).await;

    let RunNow::Ran(body) = outcome else {
        anyhow::bail!("expected a pass, got {outcome:?}");
    };
    let expected = i64::try_from(events.len())?;
    assert_eq!(graded_count(&harness, &events).await?, expected);
    assert!(
        body["graded"]
            .as_u64()
            .is_some_and(|n| n >= events.len() as u64),
        "{body}"
    );
    harness.cleanup().await
}
