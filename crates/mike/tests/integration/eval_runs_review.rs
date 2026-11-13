use chrono::Utc;
use mike::store::persistence::dossier_repo::DossierPersist as _;
use mike::store::persistence::eval_repo::EvalPersist as _;
use mike::store::persistence::eval_runs::{
    Disagreement, ItemLabelFilter, NewEvalRun, RunItemWrite, RunItemsFilter, RunItemsPage,
    RunsFilter,
};
use serde_json::{Value, json};

use crate::eval_labels::{Api, SPA, api};
use crate::turn_facts::{
    Fixture, NewTurn, Req, dev_split, fixture, insert_request, insert_turn, sample_row,
    test_import_batch,
};

struct Seeded {
    run_id: i64,
    a: i64,
    b: i64,
    c: i64,
    a_reaction_label: i64,
}

async fn bundle(f: &Fixture) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO eval.bundles (role, subject_key, origin, status, components, content_sha256, build_sha)
         VALUES ('turn_judge', '', 'candidate', 'candidate', '{}'::jsonb, 'sha-review', 'build') RETURNING id",
    )
    .fetch_one(f.handle.pool())
    .await?)
}

async fn run(f: &Fixture, bundle_id: i64) -> anyhow::Result<i64> {
    let tiers = vec!["seed".to_owned(), "gold".to_owned()];
    Ok(f.handle
        .eval()
        .create_eval_run(&NewEvalRun {
            purpose: "measure",
            bundle_id,
            rubric_version: "v2-2026-09-23",
            input_builder_version: "judge-input-v2.1",
            model: "m",
            split: "dev",
            tiers: &tiers,
            holdout_batch: None,
            labels_as_of: Utc::now(),
            label_count: 3,
            label_ids_hash: "h",
            repeat_of_run_id: None,
            max_usd: 1.0,
            argv: json!({}),
        })
        .await?)
}

async fn turn(f: &Fixture, id: i64, text: Option<&str>) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(id, id, "do it")).await?;
    let t = NewTurn {
        final_text: text,
        ..NewTurn::closed(id, id, id, id + 5)
    };
    let turn_id = insert_turn(f, &t).await?;
    dev_split(f, turn_id).await?;
    Ok(turn_id)
}

async fn seed_label(
    f: &Fixture,
    turn_id: i64,
    axis: &str,
    label: &str,
    severity: &str,
) -> anyhow::Result<i64> {
    let batch = test_import_batch(f).await?;
    Ok(sqlx::query_scalar(
        "INSERT INTO eval.labels (subject_kind, subject_id, operator_turn_id, session_id, session_uuid, axis,
                                  label, tier, method, split, source, severity, evidence, import_batch_id)
         SELECT 'turn', t.id, t.id, t.session_id, s.session_uuid, $2, $3, 'seed',
                'double_blind_adjudicated', 'dev', 'test', $4, 'adjudicated evidence', $5
           FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
          WHERE t.id = $1
         RETURNING id",
    )
    .bind(turn_id)
    .bind(axis)
    .bind(label)
    .bind(severity)
    .bind(batch)
    .fetch_one(f.handle.pool())
    .await?)
}

async fn verdict(
    f: &Fixture,
    turn_id: i64,
    bundle_id: i64,
    reaction: &str,
    delivery: &str,
) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO memory.turn_verdicts (operator_turn_id, session_id, mode, bundle_id, rubric_version,
             input_builder_version, model, reaction, delivery, confidence, rationale, unmet_ask,
             evidence_quality, as_of, input_text)
         SELECT t.id, t.session_id, 'eval', $2, 'v2', 'judge-input-v2.1', 'm', $3, $4, 0.95,
                'the stored rationale', '', 'verified', now(), 'input'
           FROM memory.operator_turns t WHERE t.id = $1
         RETURNING id",
    )
    .bind(turn_id)
    .bind(bundle_id)
    .bind(reaction)
    .bind(delivery)
    .fetch_one(f.handle.pool())
    .await?)
}

struct Judged<'a> {
    turn: i64,
    gold: (&'a str, &'a str),
    judged: (&'a str, &'a str),
    severity: &'a str,
}

async fn judged(f: &Fixture, run_id: i64, bundle_id: i64, j: &Judged<'_>) -> anyhow::Result<i64> {
    let r = seed_label(f, j.turn, "reaction", j.gold.0, j.severity).await?;
    let d = seed_label(f, j.turn, "delivery", j.gold.1, j.severity).await?;
    let v = verdict(f, j.turn, bundle_id, j.judged.0, j.judged.1).await?;
    f.handle
        .eval()
        .upsert_eval_run_item(&RunItemWrite {
            run_id,
            operator_turn_id: j.turn,
            reaction_label_id: Some(r),
            delivery_label_id: Some(d),
            turn_verdict_id: Some(v),
            outcome: "judged",
            stratum: "all",
            inclusion_prob: None,
            reaction_correct: Some(j.gold.0 == j.judged.0),
            delivery_correct: Some(j.gold.1 == j.judged.1),
        })
        .await?;
    Ok(r)
}

async fn facts(f: &Fixture, a: i64, b: i64) -> anyhow::Result<()> {
    let mut resolved = sample_row(a, f.session_id, Utc::now());
    resolved.next_state = "resolved".to_owned();
    resolved.next_genuine_turn_id = Some(b);
    resolved.next_genuine_request_id = Some(b);
    resolved.text_chars = 4;
    resolved.text_cut_at = 4;
    let mut settled = sample_row(b, f.session_id, Utc::now());
    settled.next_state = "none_session_settled".to_owned();
    settled.next_state_at_close = "none_session_settled".to_owned();
    settled.text_chars = 50;
    f.handle
        .dossier()
        .upsert_turn_facts(&[resolved, settled])
        .await?;
    Ok(())
}

async fn seed(f: &Fixture) -> anyhow::Result<Seeded> {
    let bundle_id = bundle(f).await?;
    let run_id = run(f, bundle_id).await?;
    let a = turn(f, 501, Some("done")).await?;
    let b = turn(f, 502, None).await?;
    let c = turn(f, 503, Some("half")).await?;
    let a_reaction_label = judged(
        f,
        run_id,
        bundle_id,
        &Judged {
            turn: a,
            gold: ("corrected", "delivered"),
            judged: ("redirected", "delivered"),
            severity: "severe",
        },
    )
    .await?;
    judged(
        f,
        run_id,
        bundle_id,
        &Judged {
            turn: b,
            gold: ("accepted", "partial"),
            judged: ("accepted", "shipped"),
            severity: "minor",
        },
    )
    .await?;
    f.handle
        .eval()
        .upsert_eval_run_item(&RunItemWrite {
            run_id,
            operator_turn_id: c,
            reaction_label_id: None,
            delivery_label_id: None,
            turn_verdict_id: None,
            outcome: "skipped_unresolved",
            stratum: "all",
            inclusion_prob: None,
            reaction_correct: None,
            delivery_correct: None,
        })
        .await?;
    facts(f, a, b).await?;
    Ok(Seeded {
        run_id,
        a,
        b,
        c,
        a_reaction_label,
    })
}

async fn page(f: &Fixture, run_id: i64, filter: RunItemsFilter) -> anyhow::Result<RunItemsPage> {
    let filter = RunItemsFilter {
        limit: if filter.limit == 0 { 50 } else { filter.limit },
        ..filter
    };
    Ok(f.handle.eval().eval_run_items_page(run_id, &filter).await?)
}

fn turns(p: &RunItemsPage) -> Vec<i64> {
    p.items.iter().map(|i| i.operator_turn_id).collect()
}

#[tokio::test]
async fn reaction_and_delivery_correctness_filter_independently() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let s = seed(&f).await?;
    let reaction = RunItemsFilter {
        reaction_correct: Some(false),
        ..RunItemsFilter::default()
    };
    assert_eq!(turns(&page(&f, s.run_id, reaction).await?), vec![s.a]);
    let delivery = RunItemsFilter {
        delivery_correct: Some(false),
        ..RunItemsFilter::default()
    };
    assert_eq!(turns(&page(&f, s.run_id, delivery).await?), vec![s.b]);
    let any = RunItemsFilter {
        disagreement: Some(Disagreement::Any),
        ..RunItemsFilter::default()
    };
    let all = page(&f, s.run_id, any.clone()).await?;
    assert_eq!(turns(&all), vec![s.a, s.b]);
    assert_eq!(all.total, 2);
    let second = RunItemsFilter {
        limit: 1,
        offset: 1,
        ..any
    };
    let paged = page(&f, s.run_id, second).await?;
    assert_eq!((turns(&paged), paged.total), (vec![s.b], 2));
    let skipped = RunItemsFilter {
        outcome: Some("skipped_unresolved".to_owned()),
        ..RunItemsFilter::default()
    };
    assert_eq!(turns(&page(&f, s.run_id, skipped).await?), vec![s.c]);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn label_and_severity_filters_match_run_gold_and_the_evaluated_verdict() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let s = seed(&f).await?;
    let by = |labels: ItemLabelFilter| RunItemsFilter {
        labels,
        include_review: true,
        ..RunItemsFilter::default()
    };
    let gold = by(ItemLabelFilter {
        gold_reaction: Some("corrected".to_owned()),
        ..ItemLabelFilter::default()
    });
    let hit = page(&f, s.run_id, gold).await?;
    assert_eq!(turns(&hit), vec![s.a]);
    assert_eq!(hit.items[0].gold_severity.as_deref(), Some("severe"));
    assert_eq!(hit.items[0].current_reaction.as_deref(), Some("corrected"));
    let judged = by(ItemLabelFilter {
        verdict_delivery: Some("shipped".to_owned()),
        ..ItemLabelFilter::default()
    });
    assert_eq!(turns(&page(&f, s.run_id, judged).await?), vec![s.b]);
    let severity = by(ItemLabelFilter {
        severity: Some("minor".to_owned()),
        ..ItemLabelFilter::default()
    });
    assert_eq!(turns(&page(&f, s.run_id, severity).await?), vec![s.b]);
    f.db.cleanup().await?;
    Ok(())
}

async fn get(api: &Api, route: &str) -> anyhow::Result<(u16, Value)> {
    let response = reqwest::get(format!("{}{route}", api.base)).await?;
    let status = response.status().as_u16();
    Ok((status, response.json().await.unwrap_or(Value::Null)))
}

fn review_body(run_id: i64, turn: i64, reaction: &str, note: &str) -> Value {
    json!({
        "subject": {"kind": "turn", "turn_id": turn},
        "labels": [{"axis": "reaction", "label": reaction}, {"axis": "delivery", "label": "delivered"}],
        "note": note,
        "context": "eval_review",
        "eval_run_id": run_id,
    })
}

#[tokio::test]
async fn a_review_is_an_append_only_gold_label_that_leaves_the_run_untouched() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let s = seed(&f).await?;
    let api = api(&f).await?;
    let confirm = review_body(s.run_id, s.a, "corrected", "gold stands");
    let (status, _) = api.post("/api/eval/labels", confirm.clone(), None).await?;
    assert_eq!(status, 403);
    let (status, _) = api.post("/api/eval/labels", confirm, Some(SPA)).await?;
    assert_eq!(status, 201);
    let outside = review_body(s.run_id, s.c + 1000, "corrected", "");
    let (status, _) = api.post("/api/eval/labels", outside, Some(SPA)).await?;
    assert_eq!(status, 400);
    let route = format!(
        "/api/eval/runs/{}/items?reviewed=true&include=review",
        s.run_id
    );
    let (_, reviewed) = get(&api, &route).await?;
    assert_eq!(reviewed["total"], 1);
    assert_eq!(reviewed["reviewed"], 1);
    assert_eq!(reviewed["items"][0]["operator_turn_id"], s.a);
    let change = review_body(s.run_id, s.a, "redirected", "judge was right");
    let (status, _) = api.post("/api/eval/labels", change, Some(SPA)).await?;
    assert_eq!(status, 201);
    let route = format!(
        "/api/eval/runs/{}/items?gold_reaction=corrected&include=review",
        s.run_id
    );
    let (_, changed) = get(&api, &route).await?;
    assert_eq!(changed["items"][0]["gold_reaction"], "corrected");
    assert_eq!(changed["items"][0]["current_reaction"], "redirected");
    assert_eq!(changed["items"][0]["reaction_label_id"], s.a_reaction_label);
    let route = format!("/api/eval/runs/{}/items/{}/review", s.run_id, s.a);
    let (_, detail) = get(&api, &route).await?;
    let reviews = detail["labels"]["reviews"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(reviews.len(), 4);
    assert_eq!(reviews[0]["provenance"]["eval_run_id"], s.run_id);
    assert_eq!(reviews[0]["note"], "judge was right");
    api.stop();
    f.db.cleanup().await?;
    Ok(())
}

async fn repeat_run(f: &Fixture, run_id: i64, turn: i64) -> anyhow::Result<i64> {
    let bundle_id: i64 = sqlx::query_scalar("SELECT bundle_id FROM eval.runs WHERE id = $1")
        .bind(run_id)
        .fetch_one(f.handle.pool())
        .await?;
    let repeat = run(f, bundle_id).await?;
    sqlx::query(
        "INSERT INTO eval.run_items (run_id, operator_turn_id, reaction_label_id, delivery_label_id,
             turn_verdict_id, outcome, stratum, inclusion_prob, reaction_correct, delivery_correct)
         SELECT $1, operator_turn_id, reaction_label_id, delivery_label_id, turn_verdict_id, outcome,
                stratum, inclusion_prob, reaction_correct, delivery_correct
           FROM eval.run_items WHERE run_id = $2 AND operator_turn_id = $3",
    )
    .bind(repeat)
    .bind(run_id)
    .bind(turn)
    .execute(f.handle.pool())
    .await?;
    Ok(repeat)
}

#[tokio::test]
async fn a_review_counts_in_every_run_that_holds_the_turn() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let s = seed(&f).await?;
    let repeat = repeat_run(&f, s.run_id, s.a).await?;
    let api = api(&f).await?;
    let answer = review_body(s.run_id, s.a, "redirected", "");
    let (status, _) = api.post("/api/eval/labels", answer, Some(SPA)).await?;
    assert_eq!(status, 201);
    let route = format!("/api/eval/runs/{repeat}/items?disagreement=reaction&include=review");
    let (_, items) = get(&api, &route).await?;
    assert_eq!(items["total"], 1);
    assert_eq!(items["reviewed"], 1);
    assert!(items["items"][0]["reviewed_at"].is_string());
    assert!(items["items"][0]["session_uuid"].is_string());
    let route = format!("/api/eval/runs/{repeat}/items/{}/review", s.a);
    let (_, detail) = get(&api, &route).await?;
    let reviews = detail["labels"]["reviews"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(reviews.len(), 2);
    assert_eq!(reviews[0]["provenance"]["eval_run_id"], s.run_id);
    api.stop();
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn review_detail_names_every_evidence_state_explicitly() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let s = seed(&f).await?;
    let api = api(&f).await?;
    let (status, a) = get(
        &api,
        &format!("/api/eval/runs/{}/items/{}/review", s.run_id, s.a),
    )
    .await?;
    assert_eq!(status, 200);
    assert_eq!(a["context"]["prompt_state"], "present");
    assert_eq!(a["context"]["assistant_state"], "present");
    assert_eq!(a["context"]["next_message_state"], "present");
    assert_eq!(a["context"]["next_genuine_text"], "prompt");
    assert_eq!(a["item"]["verdict"]["rationale"], "the stored rationale");
    assert_eq!(a["labels"]["run_gold"][0]["axis"], "reaction");
    assert_eq!(a["labels"]["run_gold"][0]["severity"], "severe");
    assert_eq!(
        a["labels"]["run_gold"][0]["evidence"],
        "adjudicated evidence"
    );
    let (_, b) = get(
        &api,
        &format!("/api/eval/runs/{}/items/{}/review", s.run_id, s.b),
    )
    .await?;
    assert_eq!(b["context"]["assistant_state"], "missing");
    assert_eq!(b["context"]["next_message_state"], "none");
    let (_, c) = get(
        &api,
        &format!("/api/eval/runs/{}/items/{}/review", s.run_id, s.c),
    )
    .await?;
    assert_eq!(c["item"]["outcome"], "skipped_unresolved");
    assert_eq!(c["context"]["next_message_state"], "unknown");
    let (status, _) = get(
        &api,
        &format!("/api/eval/runs/{}/items/{}/review", s.run_id, s.c + 1000),
    )
    .await?;
    assert_eq!(status, 404);
    api.stop();
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn runs_list_filters_on_the_bundle_role() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let s = seed(&f).await?;
    let by_role = |role: &str| RunsFilter {
        role: Some(role.to_owned()),
        limit: 10,
        ..RunsFilter::default()
    };
    let (turn_runs, _) = f
        .handle
        .eval()
        .list_eval_runs(&by_role("turn_judge"))
        .await?;
    assert_eq!(
        turn_runs.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![s.run_id]
    );
    let (session_runs, total) = f
        .handle
        .eval()
        .list_eval_runs(&by_role("session_judge"))
        .await?;
    assert!(session_runs.is_empty());
    assert_eq!(total, 0);
    f.db.cleanup().await?;
    Ok(())
}
