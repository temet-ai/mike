use crate::store::persistence::knowledge_repo::KnowledgePersist as _;

use serde_json::{Value, json};
use uuid::Uuid;

use crate::api::cross_crate_tests::rerank_nightly_worker_tests::{
    AlwaysIdle, at, graded_count, seeded_documents, tuning_on, with_settings_service,
};
use crate::api::rerank_search::harness::Harness;
use crate::rerank_nightly::first_ten::{FIRST_TEN, first_ten_references, relevance, turn_query};
use crate::rerank_nightly::tune::{Tuned, targets};
use crate::rerank_nightly::{Pass, Tick, night_pass, quiet_budget, run_tick};
use crate::store::persistence::search_events::NewSearchEvent;
use crate::store::persistence::search_grades::NewGrade;
use crate::store::persistence::test_support::seed_session_turns;

const PROMPT: &str = "why does the cache keep missing after a deploy";

async fn session_with_turn(harness: &Harness, project: i64) -> anyhow::Result<Uuid> {
    let session = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, now(), now())",
    )
    .bind(session)
    .bind(project)
    .execute(harness.handle.pool())
    .await?;
    seed_session_turns(harness.handle.pool(), &session.to_string(), project, 1).await?;
    sqlx::query(
        "UPDATE memory.operator_turns t SET user_prompt = $2
           FROM sessions.sessions s WHERE s.id = t.session_id AND s.session_uuid = $1",
    )
    .bind(session)
    .bind(PROMPT)
    .execute(harness.handle.pool())
    .await?;
    sqlx::query(
        "UPDATE requests.requests r SET operator_turn_id = t.id
           FROM memory.operator_turns t
          WHERE t.start_request_id = r.id
            AND r.session_id = (SELECT id FROM sessions.sessions WHERE session_uuid = $1)",
    )
    .bind(session)
    .execute(harness.handle.pool())
    .await?;
    Ok(session)
}

async fn unused_search(
    harness: &Harness,
    session: Option<Uuid>,
    returned: &Value,
) -> anyhow::Result<i64> {
    let id = harness
        .handle
        .knowledge()
        .record_search_event(&NewSearchEvent {
            session,
            tool: "search_project_knowledge",
            query: "cache",
            kind_filter: None,
            returned,
            rerank_state: "reranked",
        })
        .await?;
    sqlx::query("UPDATE memory.search_events SET used_final_at = now() WHERE id = $1")
        .bind(id)
        .execute(harness.handle.pool())
        .await?;
    Ok(id)
}

#[tokio::test]
async fn night_pass_grades_a_search_without_a_used_result_and_scores_its_first_ten()
-> anyhow::Result<()> {
    let harness = Harness::boot().await?;
    harness.enable_rerank(2_000);
    harness.reranker_prefers_later_documents(None).await;
    harness
        .state
        .rerank_service()
        .apply_setting("rerank.rescue_score", &json!(0.025));
    let (project, references) = seeded_documents(&harness).await?;
    anyhow::ensure!(references.len() == 3, "three documents must match");
    let session = session_with_turn(&harness, project).await?;
    let returned = json!([
        {"reference": references[0], "score": 0.9},
        {"reference": references[2], "score": 0.8},
        {"reference": "lesson:gone", "score": 0.1}
    ]);
    let event = unused_search(&harness, Some(session), &returned).await?;
    let replay = unused_search(&harness, None, &returned).await?;
    let budget = quiet_budget(&harness.state);

    let pass = night_pass(
        &harness.state,
        &harness.handle,
        &budget,
        at(23),
        &AlwaysIdle,
    )
    .await;

    assert_eq!(pass.graded, 1);
    assert_eq!(graded_count(&harness, &[event]).await?, 1);
    assert_eq!(graded_count(&harness, &[replay]).await?, 0);
    let (first_ten, prompt, relevant, used): (Value, Option<String>, Vec<String>, Vec<String>) =
        sqlx::query_as(
            "SELECT first_ten, turn_prompt, relevant_references, used_references
               FROM memory.search_grades WHERE event_id = $1",
        )
        .bind(event)
        .fetch_one(harness.handle.pool())
        .await?;
    assert!(used.is_empty());
    assert_eq!(prompt.as_deref(), Some(PROMPT));
    assert_eq!(relevant, vec![references[2].clone()]);
    assert_eq!(first_ten.as_array().map(Vec::len), Some(3));
    assert_eq!(first_ten[0]["position"], json!(1));
    assert_eq!(first_ten[0]["relevant"], json!(false));
    assert_eq!(first_ten[1]["position"], json!(2));
    assert!(
        first_ten[1]["query_score"]
            .as_f64()
            .is_some_and(|s| (s - 0.03).abs() < 1e-6)
    );
    assert_eq!(first_ten[1]["relevant"], json!(true));
    assert!(first_ten[1]["turn_score"].is_number());
    assert_eq!(first_ten[2]["relevant"], Value::Null);
    let sent = harness.rerank_requests().await?;
    let turn_call = sent
        .iter()
        .find(|request| {
            request["query"]
                .as_str()
                .is_some_and(|q| q.contains("Asked in the turn: ") && q.contains(PROMPT))
        })
        .ok_or_else(|| anyhow::anyhow!("no rerank call carried the turn's prompt: {sent:?}"))?;
    assert_eq!(turn_call["documents"].as_array().map(Vec::len), Some(2));
    harness.cleanup().await
}

async fn grade_relevant_only(harness: &Harness, events: &[i64]) -> anyhow::Result<()> {
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
    let relevant = vec!["turn:t".to_owned()];
    let first_ten = json!([{"reference": "turn:t", "position": 7, "query_score": 0.4,
        "turn_score": null, "relevant": true}]);
    for event in events {
        harness
            .handle
            .knowledge()
            .insert_search_grade(&NewGrade {
                event_id: *event,
                candidates: &candidates,
                used_references: &[],
                plain_positions: &[],
                reranked_positions: &[],
                first_ten: &first_ten,
                turn_prompt: Some(PROMPT),
                relevant_references: &relevant,
            })
            .await?;
    }
    Ok(())
}

#[tokio::test]
async fn run_tick_tunes_on_searches_graded_relevant_without_a_used_result_and_records_the_run()
-> anyhow::Result<()> {
    let mut harness = Harness::boot().await?;
    with_settings_service(&mut harness).await?;
    harness.enable_rerank(2_000);
    tuning_on(&harness);
    seeded_documents(&harness).await?;
    let mut events = Vec::new();
    for _ in 0..24 {
        events.push(unused_search(&harness, None, &json!([])).await?);
    }
    grade_relevant_only(&harness, &events).await?;

    let tick = run_tick(&harness.state, &harness.handle, at(23)).await;

    let Tick::Ran(Pass {
        tuned: Some(Tuned::Recorded { kept, tried }),
        ..
    }) = tick
    else {
        anyhow::bail!("the tick must tune, got {tick:?}");
    };
    assert_eq!((kept, tried), (1, 5));
    let passes: Vec<(String, i32, i32, i32)> = sqlx::query_as(
        "SELECT outcome, graded_events, settings_tried, settings_changed
           FROM memory.search_tuning_passes ORDER BY id",
    )
    .fetch_all(harness.handle.pool())
    .await?;
    assert_eq!(passes, vec![("changed".to_owned(), 24, 5, 1)]);
    let linked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.search_tuning_runs r
           JOIN memory.search_tuning_passes p ON p.id = r.pass_id",
    )
    .fetch_one(harness.handle.pool())
    .await?;
    assert_eq!(linked, 5);
    harness.cleanup().await
}

#[test]
fn first_ten_references_keeps_the_returned_order_and_stops_at_ten() {
    let returned = Value::Array(
        (0..14)
            .map(|i| json!({"reference": format!("lesson:{i}"), "score": 1.0}))
            .collect(),
    );

    let references = first_ten_references(&returned);

    assert_eq!(references.len(), FIRST_TEN);
    assert_eq!(references[0], "lesson:0");
    assert_eq!(references[9], "lesson:9");
    assert!(first_ten_references(&json!({"not": "an array"})).is_empty());
}

#[test]
fn relevance_needs_one_score_at_the_threshold_and_is_unknown_without_scores() {
    assert_eq!(relevance(None, None, -1.0), None);
    assert_eq!(relevance(Some(-2.0), None, -1.0), Some(false));
    assert_eq!(relevance(Some(-2.0), Some(-1.0), -1.0), Some(true));
    assert_eq!(relevance(Some(0.5), None, -1.0), Some(true));
}

#[test]
fn turn_query_puts_the_query_first_and_needs_a_prompt() {
    assert_eq!(turn_query("cache", None), None);
    assert_eq!(turn_query("cache", Some("   ")), None);
    assert_eq!(
        turn_query("cache", Some(" why miss ")).as_deref(),
        Some("cache\n\nAsked in the turn: why miss")
    );
}

#[test]
fn targets_are_the_used_references_then_the_new_relevant_ones() {
    let used = vec!["turn:1".to_owned(), "lesson:a".to_owned()];
    let relevant = vec!["lesson:a".to_owned(), "note:b".to_owned()];

    assert_eq!(
        targets(used, relevant),
        vec![
            "turn:1".to_owned(),
            "lesson:a".to_owned(),
            "note:b".to_owned()
        ]
    );
}
