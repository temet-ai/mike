use chrono::{Duration, Utc};
use mike::eval_metrics::EVAL_ROLLUP_SQL_VERSION;
use mike::store::persistence::cross_session::{ThresholdPassRunWrite, ThresholdWrite};
use mike::store::persistence::eval_metrics::RollupRequest;
use serde_json::json;

use crate::turn_facts::{Fixture, NewTurn, Req, fixture, insert_request, insert_turn};

struct Labelled<'a> {
    question: Option<&'a str>,
    mode: &'a str,
    status: &'a str,
    source: &'a str,
    outcome: &'a str,
    confidence: f64,
}

async fn steerer_id(f: &Fixture, name: &str) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM steering.steerers WHERE name = $1 AND deleted_at IS NULL",
    )
    .bind(name)
    .fetch_one(f.handle.pool())
    .await?)
}

async fn turn(f: &Fixture, start: i64) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(start, 0, "run the tests")).await?;
    insert_turn(f, &NewTurn::closed(start, start, 0, 30)).await
}

async fn intervention(f: &Fixture, steerer: i64) -> anyhow::Result<uuid::Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO steering.interventions
                (steerer_id, session_id, verdict_id, mode_id, authority_id, uuid, expires_at,
                 payload_text, pushed_at)
         SELECT $1, $2, v.id, m.id, a.id, gen_random_uuid(), now() + interval '10 minutes',
                'a delivered note', now()
           FROM steering.verdicts v CROSS JOIN steering.modes m CROSS JOIN steering.authorities a
          WHERE v.code = 'fired' AND m.code = 'augment' AND a.code = 'redirect'
         RETURNING uuid",
    )
    .bind(steerer)
    .bind(f.session_id)
    .fetch_one(f.handle.pool())
    .await?)
}

async fn labelled_evaluation(
    f: &Fixture,
    steerer: i64,
    turn_id: i64,
    l: &Labelled<'_>,
) -> anyhow::Result<()> {
    let delivered = if l.source == "session_reply" {
        Some(intervention(f, steerer).await?)
    } else {
        None
    };
    let verdict = match l.question {
        Some(q) => json!({"gates": {q: {"open": true}}, "questions": {q: {"applies": true}}}),
        None => json!({}),
    };
    sqlx::query(
        "INSERT INTO steering.evaluations
            (steerer_id, session_id, operator_turn_id, status, mode, question, trigger_point,
             dispatched_at, resolved_at, verdict_json, confidence, outcome_label, label_source,
             labelled_at, label_evidence, intervention_uuid)
         VALUES ($1, $2, $3, $4, $5, $6, 'turn_end', now(), now(), $7, $8, $9, $10, now(), '{}'::jsonb, $11)",
    )
    .bind(steerer)
    .bind(f.session_id)
    .bind(turn_id)
    .bind(l.status)
    .bind(l.mode)
    .bind(l.question)
    .bind(verdict)
    .bind(l.confidence)
    .bind(l.outcome)
    .bind(l.source)
    .bind(delivered)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

fn tomorrow() -> chrono::NaiveDate {
    Utc::now().date_naive() + Duration::days(1)
}

fn write(steerer: i64, decision: &str, applied: f32, candidate: Option<f32>) -> ThresholdWrite {
    ThresholdWrite {
        steerer_id: steerer,
        question: "fire".to_owned(),
        scope: "global".to_owned(),
        project_id: None,
        rollup_id: 1,
        rollup_finished_at: Utc::now(),
        window_start: Utc::now() - Duration::days(30),
        window_end: Utc::now(),
        label_basis: "weighted_by_source".to_owned(),
        question_mode: "append".to_owned(),
        threshold_mode: "apply".to_owned(),
        n_labelled: 80,
        precision_bar: 0.75,
        threshold_floor: 0.6,
        threshold_ceiling: 0.95,
        static_threshold: 0.7,
        candidate_threshold: candidate,
        applied_threshold: applied,
        decision: decision.to_owned(),
        detail: json!({"previous_threshold": 0.7, "bins": [], "steerer": "verifier"}),
    }
}

#[tokio::test]
async fn guard_rollup_counts_session_reply_labels_and_splits_each_bin_by_source()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let guard = steerer_id(&f, "turn-guard").await?;
    let base = |mode, status, source, outcome| Labelled {
        question: Some("relapse"),
        mode,
        status,
        source,
        outcome,
        confidence: 0.96,
    };
    let seeds = [
        base("shadow", "resolved_correction", "next_message", "right"),
        base("shadow", "resolved_correction", "session_end", "right"),
        base("live", "resolved_correction", "session_reply", "right"),
        base("live", "resolved_correction", "session_reply", "wrong"),
        base("live", "resolved_correction", "next_message", "right"),
    ];
    for (i, seed) in seeds.iter().enumerate() {
        let t = turn(&f, 100 + i64::try_from(i)?).await?;
        labelled_evaluation(&f, guard, t, seed).await?;
    }
    let report = f
        .handle
        .run_eval_rollup(RollupRequest {
            for_day: tomorrow(),
            mode: "manual",
            sql_version: EVAL_ROLLUP_SQL_VERSION,
            execute: true,
        })
        .await?;
    let Some(rollup_id) = report.rollup_id else {
        anyhow::bail!("rollup was not executed");
    };
    let all_row = |mode: &'static str, dimension: &'static str, value: &'static str| {
        sqlx::query_as::<_, (i32, i32, serde_json::Value)>(
            "SELECT k, n, detail FROM eval.rates
              WHERE rollup_id = $1 AND family = 'steer_calibration' AND metric = 'precision'
                AND question = 'relapse' AND mode = $2 AND dimension = $3
                AND dimension_value = $4 AND project_id IS NULL",
        )
        .bind(rollup_id)
        .bind(mode)
        .bind(dimension)
        .bind(value)
        .fetch_one(f.handle.pool())
    };
    let (k, n, detail) = all_row("live", "all", "all").await?;
    assert_eq!(
        (k, n),
        (1, 2),
        "both session_reply labels count; the live next_message label is not a basis"
    );
    assert_eq!(
        detail["by_source"]["session_reply"],
        json!({"k": 1, "n": 2})
    );
    assert_eq!(detail["by_source"]["next_message"], json!({"k": 0, "n": 0}));
    assert_eq!(detail["label_basis"]["session_reply"], 2);
    let (k, n, detail) = all_row("shadow", "all", "all").await?;
    assert_eq!((k, n), (2, 2));
    assert_eq!(detail["by_source"]["next_message"], json!({"k": 1, "n": 1}));
    assert_eq!(detail["by_source"]["session_end"], json!({"k": 1, "n": 1}));
    assert_eq!(detail["by_source"]["operator"], json!({"k": 0, "n": 0}));
    let (_, _, bin) = all_row("live", "confidence_bin_05", "0.95").await?;
    assert_eq!(bin["by_source"]["session_reply"], json!({"k": 1, "n": 2}));
    Ok(())
}

#[tokio::test]
async fn rollup_as_of_skips_a_newer_rollup_with_no_calibration_rows() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let insert_rollup = |days: i32, hours_ago: i32| {
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO eval.rollups (for_day, sql_version, mode, labels_as_of, finished_at)
             VALUES (current_date - $1, 'jc18-v1', 'manual', now(), now() - make_interval(hours => $2))
             RETURNING id",
        )
        .bind(days)
        .bind(hours_ago)
        .fetch_one(f.handle.pool())
    };
    let calibrated = insert_rollup(2, 30).await?;
    let action_effect_only = insert_rollup(1, 1).await?;
    sqlx::query(
        "INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind, window_end, role,
                                 axis, class, k, n)
         VALUES ($1, 'steer_calibration', 'fire_rate', 'steerer_evaluations', 'global', now(),
                 'steerer', 'steer_valid', 'refocus', 0, 0),
                ($2, 'action_effect', 'share', 'before', 'global', now(), 'steerer', 'x', 'y', 0, 0)",
    )
    .bind(calibrated)
    .bind(action_effect_only)
    .execute(f.handle.pool())
    .await?;
    let seen = f.handle.rollup_as_of(Utc::now()).await?;
    assert_eq!(seen.map(|r| r.id), Some(calibrated));
    Ok(())
}

#[tokio::test]
async fn fire_bins_group_verifier_labels_by_confidence_edge_and_source() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let verifier = steerer_id(&f, "verifier").await?;
    let seeds = [
        ("session_reply", "right", 0.96),
        ("session_reply", "right", 0.97),
        ("session_reply", "wrong", 0.95),
        ("next_message", "right", 0.91),
    ];
    for (i, (source, outcome, confidence)) in seeds.iter().enumerate() {
        let t = turn(&f, 200 + i64::try_from(i)?).await?;
        labelled_evaluation(
            &f,
            verifier,
            t,
            &Labelled {
                question: None,
                mode: "live",
                status: "resolved_correction",
                source,
                outcome,
                confidence: *confidence,
            },
        )
        .await?;
    }
    let bins = f
        .handle
        .fetch_fire_bins(Utc::now() + Duration::minutes(1))
        .await?;
    let find = |edge: &str, source: &str| {
        bins.iter()
            .find(|b| b.steerer_id == verifier && b.lower_edge == edge && b.label_source == source)
            .map(|b| (b.k, b.n))
    };
    assert_eq!(find("0.95", "session_reply"), Some((2, 3)));
    assert_eq!(find("0.90", "next_message"), Some((1, 1)));
    let steerers = f.handle.fire_steerers().await?;
    assert!(
        steerers
            .iter()
            .any(|s| s.id == verifier && s.name == "verifier")
    );
    Ok(())
}

#[tokio::test]
async fn fire_thresholds_in_force_follow_the_latest_applied_row_per_steerer() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let verifier = steerer_id(&f, "verifier").await?;
    f.handle
        .insert_threshold_rows(&[write(verifier, "applied", 0.85, Some(0.85))])
        .await?;
    f.handle
        .insert_threshold_rows(&[write(verifier, "below_bar", 0.85, None)])
        .await?;
    let in_force = f
        .handle
        .fire_thresholds_in_force(Utc::now() + Duration::minutes(1))
        .await?;
    let mine: Vec<_> = in_force
        .iter()
        .filter(|t| t.steerer_id == verifier)
        .collect();
    assert_eq!(mine.len(), 1);
    assert!((mine[0].applied_threshold - 0.85).abs() < 1e-6);
    assert_eq!(mine[0].question, "fire");
    let guard = steerer_id(&f, "turn-guard").await?;
    let guard_rows = f
        .handle
        .thresholds_in_force(guard, Utc::now() + Duration::minutes(1))
        .await?;
    assert!(guard_rows.iter().all(|t| t.question != "fire"));
    Ok(())
}

#[tokio::test]
async fn threshold_pass_run_is_recorded_once_per_rollup_and_counts_attempts() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let run = |outcome: &str, rows: i32| ThresholdPassRunWrite {
        rollup_id: 9_001,
        outcome: outcome.to_owned(),
        threshold_mode: "apply".to_owned(),
        rows_written: rows,
        applied: 0,
        n_by_source: json!({"relapse": {"session_reply": 2}}),
        detail: json!({}),
    };
    f.handle.record_threshold_pass_run(&run("error", 0)).await?;
    f.handle
        .record_threshold_pass_run(&run("wrote", 276))
        .await?;
    let runs = f.handle.recent_threshold_pass_runs(500).await?;
    let mine: Vec<_> = runs.iter().filter(|r| r.rollup_id == 9_001).collect();
    assert_eq!(mine.len(), 1);
    assert_eq!(
        (
            mine[0].outcome.as_str(),
            mine[0].attempts,
            mine[0].rows_written
        ),
        ("wrote", 2, 276)
    );
    assert_eq!(mine[0].n_by_source["relapse"]["session_reply"], 2);
    Ok(())
}

#[tokio::test]
async fn threshold_pass_run_rejects_an_unknown_outcome() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let bad = ThresholdPassRunWrite {
        rollup_id: 9_002,
        outcome: "silent".to_owned(),
        threshold_mode: "apply".to_owned(),
        rows_written: 0,
        applied: 0,
        n_by_source: json!({}),
        detail: json!({}),
    };
    assert!(f.handle.record_threshold_pass_run(&bad).await.is_err());
    Ok(())
}
