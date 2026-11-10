use chrono::{Duration, Utc};
use serde_json::json;

use super::{NewGrade, NewTuningRun};
use crate::store::persistence::search_events::fixture_tests::Fixture;

async fn finalise(fixture: &Fixture, event: i64, used: &[&str]) -> anyhow::Result<()> {
    let used: Vec<String> = used.iter().map(|u| (*u).to_owned()).collect();
    sqlx::query(
        "UPDATE memory.search_events SET used_references = $2, used_final_at = now() WHERE id = $1",
    )
    .bind(event)
    .bind(&used)
    .execute(fixture.handle.pool())
    .await?;
    Ok(())
}

fn grade<'a>(event_id: i64, candidates: &'a serde_json::Value, used: &'a [String]) -> NewGrade<'a> {
    NewGrade {
        event_id,
        candidates,
        used_references: used,
        plain_positions: &[2],
        reranked_positions: &[1],
    }
}

fn run(key: &str, kept: bool) -> NewTuningRun {
    NewTuningRun {
        setting_key: key.to_owned(),
        before: json!(1.0),
        after: json!(1.25),
        score_before: 0.4,
        score_after: 0.5,
        kept,
        reason: "test".to_owned(),
    }
}

#[tokio::test]
async fn ungraded_search_events_returns_only_finalised_events_that_have_used_references()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(2).await?;
    let ready = fixture
        .record(Some(fixture.session), "ready query", &["lesson:a"])
        .await?;
    let open = fixture
        .record(Some(fixture.session), "open query", &["lesson:a"])
        .await?;
    let unused = fixture
        .record(Some(fixture.session), "unused query", &["lesson:a"])
        .await?;
    finalise(&fixture, ready, &["lesson:a"]).await?;
    sqlx::query("UPDATE memory.search_events SET used_references = '{lesson:a}' WHERE id = $1")
        .bind(open)
        .execute(fixture.handle.pool())
        .await?;
    finalise(&fixture, unused, &[]).await?;

    let events = fixture.handle.ungraded_search_events(100).await?;

    let ids: Vec<i64> = events.iter().map(|e| e.event_id).collect();
    assert!(ids.contains(&ready));
    assert!(!ids.contains(&open));
    assert!(!ids.contains(&unused));
    let Some(event) = events.iter().find(|e| e.event_id == ready) else {
        anyhow::bail!("the finalised event must be listed");
    };
    assert_eq!(event.query, "ready query");
    assert_eq!(event.used_references, vec!["lesson:a".to_owned()]);
    fixture.cleanup().await
}

#[tokio::test]
async fn insert_search_grade_removes_the_event_from_the_ungraded_list_and_ignores_a_repeat()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(2).await?;
    let event = fixture
        .record(Some(fixture.session), "graded query", &["lesson:a"])
        .await?;
    finalise(&fixture, event, &["lesson:a"]).await?;
    let candidates = json!([{"reference": "lesson:a", "kind": "lesson", "lexical": 0.5,
        "semantic": 0.0, "rerank_score": null}]);
    let used = vec!["lesson:a".to_owned()];

    fixture
        .handle
        .insert_search_grade(&grade(event, &candidates, &used))
        .await?;
    fixture
        .handle
        .insert_search_grade(&grade(event, &candidates, &used))
        .await?;

    let ungraded = fixture.handle.ungraded_search_events(100).await?;
    assert!(ungraded.iter().all(|e| e.event_id != event));
    let stored = fixture
        .handle
        .search_grades_since(Utc::now() - Duration::hours(1))
        .await?;
    assert_eq!(stored.iter().filter(|g| g.event_id == event).count(), 1);
    fixture.cleanup().await
}

#[tokio::test]
async fn search_grades_since_leaves_out_older_grades() -> anyhow::Result<()> {
    let fixture = Fixture::boot(2).await?;
    let old = fixture
        .record(Some(fixture.session), "old query", &["lesson:a"])
        .await?;
    let new = fixture
        .record(Some(fixture.session), "new query", &["lesson:a"])
        .await?;
    let candidates = json!([]);
    let used = vec!["lesson:a".to_owned()];
    for event in [old, new] {
        fixture
            .handle
            .insert_search_grade(&grade(event, &candidates, &used))
            .await?;
    }
    sqlx::query("UPDATE memory.search_grades SET graded_at = now() - interval '30 days' WHERE event_id = $1")
        .bind(old)
        .execute(fixture.handle.pool())
        .await?;

    let stored = fixture
        .handle
        .search_grades_since(Utc::now() - Duration::days(14))
        .await?;

    let ids: Vec<i64> = stored.iter().map(|g| g.event_id).collect();
    assert!(ids.contains(&new));
    assert!(!ids.contains(&old));
    fixture.cleanup().await
}

#[tokio::test]
async fn insert_search_tuning_runs_writes_every_row_and_marks_the_window_tuned()
-> anyhow::Result<()> {
    let fixture = Fixture::boot(1).await?;
    let ran_at = Utc::now() + Duration::days(400);
    let key = format!("search.ranking.test_{}", fixture.session);

    let written = fixture
        .handle
        .insert_search_tuning_runs(&[run(&key, true), run(&key, false)], 31, ran_at)
        .await?;

    assert_eq!(written, 2);
    assert!(
        fixture
            .handle
            .search_tuned_since(ran_at - Duration::minutes(1))
            .await?
    );
    assert!(
        !fixture
            .handle
            .search_tuned_since(ran_at + Duration::hours(1))
            .await?
    );
    let rows: Vec<(bool, i32)> = sqlx::query_as(
        "SELECT kept, graded_events FROM memory.search_tuning_runs WHERE setting_key = $1 ORDER BY id",
    )
    .bind(&key)
    .fetch_all(fixture.handle.pool())
    .await?;
    assert_eq!(rows, vec![(true, 31), (false, 31)]);
    fixture.cleanup().await
}
