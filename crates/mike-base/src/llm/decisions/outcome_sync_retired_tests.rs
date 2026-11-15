use chrono::{Duration, Utc};
use serde_json::json;

use super::outcome_sync_support_tests::{Env, NewRecord, pair};
use super::retired::RETIRED_LABEL_SOURCES;

async fn label(env: &Env, record: i64, label: &str, source: &str) -> anyhow::Result<()> {
    super::store(&env.handle)
        .insert_decision_outcome(record, label, source, &json!({}))
        .await?;
    Ok(())
}

#[tokio::test]
async fn retired_sources_stay_on_record_but_the_loop_does_not_score_them() -> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    let since = Utc::now() - Duration::days(1);
    let mut retired_only_ids = Vec::new();
    for (point, source) in RETIRED_LABEL_SOURCES {
        let record = env.record(NewRecord::new(point, "sent")).await?;
        label(&env, record, "right", source).await?;
        retired_only_ids.push(record);
    }
    let both = env
        .record(NewRecord::new("verifier_precheck", "sent"))
        .await?;
    label(&env, both, "right", "turn_guard_label").await?;
    label(&env, both, "wrong", "precheck_decision").await?;
    let store = super::store(&env.handle);
    let labelled = store.labelled_decisions(since).await?;
    let counts = store.decision_point_counts(since).await?;
    let mine: Vec<_> = labelled
        .iter()
        .filter(|l| RETIRED_LABEL_SOURCES.iter().any(|(p, _)| *p == l.point))
        .collect();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].point, "verifier_precheck");
    assert_eq!(mine[0].label, "wrong");
    let precheck = counts
        .iter()
        .find(|c| c.point == "verifier_precheck")
        .map(|c| c.labelled);
    assert_eq!(precheck, Some(1));
    for record in retired_only_ids {
        assert_eq!(env.labels(record).await?.len(), 1);
    }
    assert_eq!(
        env.labels(both).await?,
        vec![
            pair("right", "turn_guard_label"),
            pair("wrong", "precheck_decision")
        ]
    );
    env.finish().await
}
