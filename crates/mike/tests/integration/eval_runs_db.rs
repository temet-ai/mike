use chrono::Utc;
use mike::store::persistence::eval_runs::{LabelSetFilter, NewEvalGate, NewEvalRun, RunItemWrite};

use crate::turn_facts::{Fixture, NewTurn, Req, fixture, insert_request, insert_turn};

async fn call_with_role(f: &Fixture, role: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO llm.calls (role, model, outcome) VALUES ($1, 'm', 'ok')")
        .bind(role)
        .execute(f.handle.pool())
        .await
        .map(|_| ())
}

#[tokio::test]
async fn llm_calls_accepts_eval_role_and_rejects_an_unknown_one() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    call_with_role(&f, "eval").await?;
    let err = call_with_role(&f, "nonsense")
        .await
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(err.contains("calls_role_check"), "{err}");
    Ok(())
}

async fn bundle(f: &Fixture) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO eval.bundles (role, subject_key, origin, status, components, content_sha256, build_sha)
         VALUES ('turn_judge', '', 'candidate', 'candidate', '{}'::jsonb, 'sha-jc10', 'build') RETURNING id",
    )
    .fetch_one(f.handle.pool())
    .await?)
}

async fn run(f: &Fixture, bundle_id: i64, split: &str, batch: Option<&str>) -> anyhow::Result<i64> {
    let tiers = vec!["gold".to_owned()];
    Ok(f.handle
        .create_eval_run(&NewEvalRun {
            purpose: "measure",
            bundle_id,
            rubric_version: "v2-2026-09-23",
            input_builder_version: "judge-input-v2.1",
            model: "m",
            split,
            tiers: &tiers,
            holdout_batch: batch,
            labels_as_of: Utc::now(),
            label_count: 1,
            label_ids_hash: "h",
            repeat_of_run_id: None,
            max_usd: 1.0,
            argv: serde_json::json!({}),
        })
        .await?)
}

async fn turn(f: &Fixture, id: i64) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(id, 0, "do it")).await?;
    insert_turn(f, &NewTurn::closed(id, id, 0, 30)).await
}

async fn gold_label(
    f: &Fixture,
    turn_id: i64,
    label: &str,
    method: &str,
    tier: &str,
) -> anyhow::Result<i64> {
    sqlx::query("INSERT INTO eval.turn_splits (operator_turn_id, split, split_reason) VALUES ($1, 'dev', 'hash') ON CONFLICT DO NOTHING")
        .bind(turn_id)
        .execute(f.handle.pool())
        .await?;
    let batch: Option<i64> = if tier == "seed" {
        Some(sqlx::query_scalar(
            "INSERT INTO eval.import_batches (source_key, manifest, counts, imported_by)
             VALUES ('test/' || gen_random_uuid()::text, '{}'::jsonb, '{}'::jsonb, 'test') RETURNING id",
        )
        .fetch_one(f.handle.pool())
        .await?)
    } else {
        None
    };
    Ok(sqlx::query_scalar(
        "INSERT INTO eval.labels (subject_kind, subject_id, operator_turn_id, session_id, session_uuid, axis,
                                  label, tier, method, split, source, import_batch_id)
         SELECT 'turn', t.id, t.id, t.session_id, s.session_uuid, 'reaction', $2, $3, $4, 'dev', 'test', $5
           FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
          WHERE t.id = $1
         RETURNING id",
    )
    .bind(turn_id)
    .bind(label)
    .bind(tier)
    .bind(method)
    .bind(batch)
    .fetch_one(f.handle.pool())
    .await?)
}

fn default_filter(labels_as_of: chrono::DateTime<Utc>) -> LabelSetFilter {
    LabelSetFilter {
        labels_as_of,
        split: "dev".to_owned(),
        tiers: vec!["seed".to_owned(), "gold".to_owned(), "relayed".to_owned()],
        methods: vec![
            "double_blind_adjudicated".to_owned(),
            "operator_click".to_owned(),
            "operator_confirmed_relay".to_owned(),
            "relayed_quote".to_owned(),
        ],
        holdout_batch: None,
    }
}

#[tokio::test]
async fn load_label_set_prefers_gold_and_excludes_single_model_labels() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let a = turn(&f, 301).await?;
    let b = turn(&f, 302).await?;
    gold_label(&f, a, "accepted", "double_blind_adjudicated", "seed").await?;
    let gold = gold_label(&f, a, "corrected", "operator_click", "gold").await?;
    gold_label(&f, b, "corrected", "single_model", "seed").await?;
    let as_of = crate::seed::database_now(f.handle.pool()).await?;
    let set = f.handle.load_eval_label_set(&default_filter(as_of)).await?;
    assert_eq!(set.len(), 1);
    assert_eq!(set[0].operator_turn_id, a);
    assert_eq!(set[0].reaction_label_id, gold);
    assert_eq!(set[0].reaction_label, "corrected");
    Ok(())
}

#[tokio::test]
async fn run_items_accept_null_inclusion_prob_and_judged_requires_a_verdict() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let bundle_id = bundle(&f).await?;
    let run_id = run(&f, bundle_id, "dev", None).await?;
    let t = turn(&f, 311).await?;
    let skipped = RunItemWrite {
        run_id,
        operator_turn_id: t,
        reaction_label_id: None,
        delivery_label_id: None,
        turn_verdict_id: None,
        outcome: "skipped_unresolved",
        stratum: "all",
        inclusion_prob: None,
        reaction_correct: None,
        delivery_correct: None,
    };
    f.handle.upsert_eval_run_item(&skipped).await?;
    let judged_without_verdict = RunItemWrite {
        outcome: "judged",
        ..skipped
    };
    let err = f
        .handle
        .upsert_eval_run_item(&judged_without_verdict)
        .await
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(err.contains("eval_run_items_judged_check"), "{err}");
    Ok(())
}

#[tokio::test]
async fn refresh_run_updates_counters_status_and_report() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let bundle_id = bundle(&f).await?;
    let run_id = run(&f, bundle_id, "dev", None).await?;
    let t = turn(&f, 321).await?;
    f.handle
        .upsert_eval_run_item(&RunItemWrite {
            run_id,
            operator_turn_id: t,
            reaction_label_id: None,
            delivery_label_id: None,
            turn_verdict_id: None,
            outcome: "refused",
            stratum: "all",
            inclusion_prob: Some(0.5),
            reaction_correct: None,
            delivery_correct: None,
        })
        .await?;
    let report = serde_json::json!({"n": 0});
    f.handle
        .refresh_eval_run(run_id, "complete", Some(&report))
        .await?;
    let row = f
        .handle
        .get_eval_run(run_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("run gone"))?;
    assert_eq!(row.status, "complete");
    assert_eq!(row.items_refused, 1);
    assert!(row.finished_at.is_some());
    assert_eq!(row.report, Some(report));
    Ok(())
}

#[tokio::test]
async fn gate_rows_count_per_holdout_batch() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let batch = "jc11/test-batch";
    let mut runs = Vec::new();
    for _ in 0..3 {
        let b = bundle(&f).await?;
        runs.push(run(&f, b, "holdout", Some(batch)).await?);
    }
    let baseline = run(&f, bundle(&f).await?, "holdout", Some(batch)).await?;
    for candidate in &runs {
        f.handle
            .insert_eval_gate(&NewEvalGate {
                candidate_run_id: *candidate,
                baseline_run_id: Some(baseline),
                noise_run_id: None,
                holdout_batch: Some(batch),
                verdict: "fail",
                floors: serde_json::json!({"rule": "promotion"}),
                report: serde_json::json!({}),
                override_reason: None,
            })
            .await?;
    }
    assert_eq!(f.handle.count_holdout_gates(batch).await?, 3);
    assert_eq!(f.handle.count_holdout_gates("other").await?, 0);
    Ok(())
}
