use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{DateTime, TimeZone, Utc};
use serde_json::{Value, json};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::grade::positions;
use super::tune::Tuned;
use super::{Idle, Pass, Stop, Tick, night_pass, quiet_budget, run_tick, window_start};
use crate::api::project_knowledge::rerank_tests::seed_project_with_documents;
use crate::api::rerank_search::harness::Harness;
use crate::config::SettingsWriteGuard;
use crate::settings::service::SettingsService;
use crate::store::persistence::search_events::NewSearchEvent;
use crate::store::persistence::search_grades::NewGrade;
use crate::store::persistence::test_support::seed_session_turns;

struct IdleOnce(AtomicUsize);

impl Idle for IdleOnce {
    async fn is_idle(&self) -> bool {
        self.0.fetch_add(1, Ordering::SeqCst) == 0
    }
}

struct AlwaysIdle;

impl Idle for AlwaysIdle {
    async fn is_idle(&self) -> bool {
        true
    }
}

fn at(hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 9, hour, 30, 0)
        .single()
        .unwrap_or_default()
}

fn tuning_on(harness: &Harness) {
    harness
        .state
        .rerank_service()
        .apply_setting("rerank.tuning.enabled", &json!(true));
}

async fn project_id(harness: &Harness, project: &str) -> anyhow::Result<i64> {
    Ok(harness.handle.resolve_knowledge_project(project).await?.id)
}

async fn seed_events(
    harness: &Harness,
    project: i64,
    used: &[&str],
    count: usize,
) -> anyhow::Result<(Uuid, Vec<i64>)> {
    let session = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, now(), now())",
    )
    .bind(session)
    .bind(project)
    .execute(harness.handle.pool())
    .await?;
    let used: Vec<String> = used.iter().map(|u| (*u).to_owned()).collect();
    let mut events = Vec::new();
    for _ in 0..count {
        let id = harness
            .handle
            .record_search_event(&NewSearchEvent {
                session: Some(session),
                tool: "search_project_knowledge",
                query: "cache",
                kind_filter: None,
                returned: &json!([]),
                rerank_state: "off",
            })
            .await?;
        sqlx::query(
            "UPDATE memory.search_events SET used_references = $2, used_final_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(&used)
        .execute(harness.handle.pool())
        .await?;
        events.push(id);
    }
    Ok((session, events))
}

async fn graded_count(harness: &Harness, events: &[i64]) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM memory.search_grades WHERE event_id = ANY($1)")
            .bind(events)
            .fetch_one(harness.handle.pool())
            .await?,
    )
}

async fn seeded_documents(harness: &Harness) -> anyhow::Result<(i64, Vec<String>)> {
    let project = seed_project_with_documents(harness).await?;
    let page = harness
        .handle
        .search_project_knowledge(
            &crate::store::persistence::project_knowledge::KnowledgeQuery {
                project: project.clone(),
                query: "cache".to_owned(),
                ..Default::default()
            },
        )
        .await?;
    let references = page["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["reference"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    Ok((project_id(harness, &project).await?, references))
}

#[tokio::test]
async fn run_tick_with_tuning_off_grades_nothing() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness.enable_rerank(2_000);
    harness.reranker_prefers_later_documents(None).await;
    let (project, references) = seeded_documents(&harness).await?;
    let (_, events) = seed_events(&harness, project, &[&references[0]], 1).await?;

    let tick = run_tick(&harness.state, &harness.handle, at(23)).await;

    assert_eq!(tick, Tick::TuningOff);
    assert_eq!(graded_count(&harness, &events).await?, 0);
    assert!(harness.rerank_requests().await?.is_empty());
    harness.cleanup().await
}

#[tokio::test]
async fn run_tick_outside_the_quiet_window_grades_nothing() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness.enable_rerank(2_000);
    harness.reranker_prefers_later_documents(None).await;
    tuning_on(&harness);
    let (project, references) = seeded_documents(&harness).await?;
    let (_, events) = seed_events(&harness, project, &[&references[0]], 1).await?;

    let tick = run_tick(&harness.state, &harness.handle, at(12)).await;

    assert_eq!(tick, Tick::OutsideWindow);
    assert_eq!(graded_count(&harness, &events).await?, 0);
    assert!(harness.rerank_requests().await?.is_empty());
    harness.cleanup().await
}

#[tokio::test]
async fn run_tick_with_recent_primary_activity_grades_nothing() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness.enable_rerank(2_000);
    harness.reranker_prefers_later_documents(None).await;
    tuning_on(&harness);
    let (project, references) = seeded_documents(&harness).await?;
    let (session, events) = seed_events(&harness, project, &[&references[0]], 1).await?;
    seed_session_turns(harness.handle.pool(), &session.to_string(), project, 1).await?;

    let tick = run_tick(&harness.state, &harness.handle, at(23)).await;

    assert_eq!(tick, Tick::Active);
    assert_eq!(graded_count(&harness, &events).await?, 0);
    assert!(harness.rerank_requests().await?.is_empty());
    harness.cleanup().await
}

#[tokio::test]
async fn run_tick_in_the_quiet_window_grades_the_search_and_records_both_orders()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness.enable_rerank(2_000);
    harness.reranker_prefers_later_documents(None).await;
    tuning_on(&harness);
    let (project, references) = seeded_documents(&harness).await?;
    anyhow::ensure!(
        references.len() == 3,
        "three documents must match the query"
    );
    let (_, events) = seed_events(&harness, project, &[&references[2]], 1).await?;

    let tick = run_tick(&harness.state, &harness.handle, at(23)).await;

    let Tick::Ran(pass) = tick else {
        anyhow::bail!("the tick must run, got {tick:?}");
    };
    assert_eq!(pass.graded, 1);
    assert!(matches!(pass.tuned, Some(Tuned::TooFewGrades(_))));
    let (plain, reranked, candidates): (Vec<i32>, Vec<i32>, Value) = sqlx::query_as(
        "SELECT plain_positions, reranked_positions, candidates
           FROM memory.search_grades WHERE event_id = $1",
    )
    .bind(events[0])
    .fetch_one(harness.handle.pool())
    .await?;
    assert_eq!(plain, vec![3]);
    assert_eq!(reranked, vec![1]);
    assert_eq!(candidates.as_array().map(Vec::len), Some(3));
    assert!(candidates[0]["rerank_score"].is_number());
    let sent = harness.rerank_requests().await?;
    assert_eq!(sent.len(), 1);
    assert!(
        sent[0]["documents"]
            .as_array()
            .is_some_and(|d| d.len() == 3)
    );
    harness.cleanup().await
}

#[tokio::test]
async fn night_pass_stops_grading_when_a_session_becomes_active_between_two_events()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness.enable_rerank(2_000);
    harness.reranker_prefers_later_documents(None).await;
    let (project, references) = seeded_documents(&harness).await?;
    let (_, events) = seed_events(&harness, project, &[&references[0]], 2).await?;
    let idle = IdleOnce(AtomicUsize::new(0));
    let budget = quiet_budget(&harness.handle).await;

    let pass = night_pass(&harness.state, &harness.handle, &budget, at(23), &idle).await;

    assert_eq!(pass.graded, 1);
    assert_eq!(pass.stopped, Some(Stop::SessionActive));
    assert_eq!(graded_count(&harness, &events).await?, 1);
    assert_eq!(graded_count(&harness, &events[..1]).await?, 1);
    harness.cleanup().await
}

#[tokio::test]
async fn night_pass_when_the_reranker_fails_leaves_the_search_ungraded() -> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness.enable_rerank(2_000);
    harness
        .reranker_answers(crate::api::rerank_search::harness::failing_answer())
        .await;
    let (project, references) = seeded_documents(&harness).await?;
    let (_, events) = seed_events(&harness, project, &[&references[0]], 2).await?;
    let budget = quiet_budget(&harness.handle).await;

    let pass = night_pass(
        &harness.state,
        &harness.handle,
        &budget,
        at(23),
        &AlwaysIdle,
    )
    .await;

    assert_eq!(pass.graded, 0);
    assert_eq!(pass.stopped, Some(Stop::RerankerFailed));
    assert_eq!(graded_count(&harness, &events).await?, 0);
    harness.cleanup().await
}

async fn grade_synthetic(harness: &Harness, events: &[i64]) -> anyhow::Result<()> {
    let mut candidates: Vec<Value> = (0..6)
        .map(|i| {
            json!({"reference": format!("lesson:{i}"), "kind": "lesson",
                "lexical": 0.9 - 0.1 * f64::from(i), "semantic": 0.0, "rerank_score": null})
        })
        .collect();
    candidates.push(
        json!({"reference": "turn:t", "kind": "turn", "lexical": 0.3,
        "semantic": 0.0, "rerank_score": null}),
    );
    let candidates = Value::Array(candidates);
    let used = vec!["turn:t".to_owned()];
    for event in events {
        harness
            .handle
            .insert_search_grade(&NewGrade {
                event_id: *event,
                candidates: &candidates,
                used_references: &used,
                plain_positions: &[7],
                reranked_positions: &[7],
            })
            .await?;
    }
    Ok(())
}

async fn with_settings_service(harness: &mut Harness) -> anyhow::Result<()> {
    harness.state.settings_service = Some(
        SettingsService::reconcile_on_startup(
            harness.state.settings_overlay.clone(),
            Arc::new(Mutex::new(SettingsWriteGuard)),
            harness.handle.clone(),
        )
        .await?,
    );
    Ok(())
}

#[tokio::test]
async fn run_tick_with_enough_grades_tunes_once_and_applies_the_kept_value() -> anyhow::Result<()> {
    let mut harness = Harness::boot().await?;
    with_settings_service(&mut harness).await?;
    harness.enable_rerank(2_000);
    tuning_on(&harness);
    let (project, _) = seeded_documents(&harness).await?;
    let (_, events) = seed_events(&harness, project, &["turn:t"], 20).await?;
    grade_synthetic(&harness, &events).await?;

    let first = run_tick(&harness.state, &harness.handle, at(23)).await;
    let second = run_tick(&harness.state, &harness.handle, at(23)).await;

    let Tick::Ran(Pass {
        tuned: Some(Tuned::Recorded { kept, tried }),
        ..
    }) = first
    else {
        anyhow::bail!("the first tick must tune, got {first:?}");
    };
    assert_eq!((kept, tried), (1, 5));
    assert!(matches!(
        second,
        Tick::Ran(Pass {
            tuned: Some(Tuned::AlreadyToday),
            ..
        })
    ));
    let ranking = harness.state.rerank_service().ranking();
    assert_eq!(ranking.kind_weight("turn"), 1.25);
    let stored: Value = sqlx::query_scalar(
        "SELECT value FROM config.settings WHERE key = 'search.ranking.kind_weights'",
    )
    .fetch_one(harness.handle.pool())
    .await?;
    assert_eq!(stored, json!({"turn": 1.25}));
    let runs: Vec<(String, bool)> =
        sqlx::query_as("SELECT setting_key, kept FROM memory.search_tuning_runs ORDER BY id")
            .fetch_all(harness.handle.pool())
            .await?;
    assert_eq!(runs.len(), 5);
    assert_eq!(runs.iter().filter(|(_, kept)| *kept).count(), 1);
    harness.cleanup().await
}

#[tokio::test]
async fn run_tick_with_too_few_grades_records_nothing() -> anyhow::Result<()> {
    let mut harness = Harness::boot().await?;
    with_settings_service(&mut harness).await?;
    harness.enable_rerank(2_000);
    tuning_on(&harness);
    let (project, _) = seeded_documents(&harness).await?;
    let (_, events) = seed_events(&harness, project, &["turn:t"], 3).await?;
    grade_synthetic(&harness, &events).await?;

    let tick = run_tick(&harness.state, &harness.handle, at(23)).await;

    assert!(matches!(
        tick,
        Tick::Ran(Pass {
            tuned: Some(Tuned::TooFewGrades(n)),
            ..
        }) if n >= 3
    ));
    assert!(harness.state.rerank_service().ranking().is_default());
    harness.cleanup().await
}

#[test]
fn window_start_is_the_latest_start_hour_not_after_now() {
    let evening = Utc.with_ymd_and_hms(2026, 11, 9, 23, 30, 0).single();
    let after_midnight = Utc.with_ymd_and_hms(2026, 11, 10, 2, 0, 0).single();
    let expected = Utc.with_ymd_and_hms(2026, 11, 9, 22, 0, 0).single();

    assert_eq!(evening.map(|now| window_start(now, 22)), expected);
    assert_eq!(after_midnight.map(|now| window_start(now, 22)), expected);
}

#[test]
fn positions_are_one_based_and_zero_when_the_reference_is_absent() {
    let ordered = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
    let used = vec!["c".to_owned(), "z".to_owned(), "a".to_owned()];

    assert_eq!(positions(&used, &ordered), vec![3, 0, 1]);
}
