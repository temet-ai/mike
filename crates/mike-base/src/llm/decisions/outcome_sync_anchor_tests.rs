use serde_json::{Value, json};
use uuid::Uuid;

use super::outcome_sync_support_tests::{Env, Turn, pair};

struct Raw<'a> {
    point: &'a str,
    mode: &'a str,
    applied: Option<&'a str>,
    state: Value,
    answers: Option<Value>,
    composed: Option<Value>,
    operator_turn_id: Option<i64>,
    request_id: Option<i64>,
    evaluation_uuid: Option<Uuid>,
}

impl<'a> Raw<'a> {
    fn new(point: &'a str) -> Self {
        Self {
            point,
            mode: "on",
            applied: None,
            state: json!({}),
            answers: None,
            composed: None,
            operator_turn_id: None,
            request_id: None,
            evaluation_uuid: None,
        }
    }
}

async fn insert(env: &Env, raw: Raw<'_>) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "INSERT INTO decisions.records
             (point, mode, question_set, question_set_hash, model_requested, state, questions,
              request_body, outcome, session_id, operator_turn_id, request_id, evaluation_uuid,
              answers, composed, applied)
         VALUES ($1, $2, 'test_v1', 'h', 'typesafe/jev-1.14', $3, '{}'::jsonb, '{}'::jsonb, 'ok',
                 $4, $5, $6, $7, $8, $9, $10)
         RETURNING id",
    )
    .bind(raw.point)
    .bind(raw.mode)
    .bind(raw.state)
    .bind(env.session_id)
    .bind(raw.operator_turn_id)
    .bind(raw.request_id)
    .bind(raw.evaluation_uuid)
    .bind(raw.answers)
    .bind(raw.composed)
    .bind(raw.applied)
    .fetch_one(env.handle.pool())
    .await?)
}

async fn intervention(env: &Env) -> anyhow::Result<Uuid> {
    Ok(sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO steering.interventions
             (steerer_id, session_id, verdict_id, mode_id, authority_id, payload_text)
         SELECT st.id, $1, v.id,
                (SELECT id FROM steering.modes ORDER BY id LIMIT 1),
                (SELECT id FROM steering.authorities ORDER BY id LIMIT 1),
                'nudge'
           FROM steering.steerers st, steering.verdicts v
          WHERE st.kind = 'verifier' AND v.code = 'fired'
          ORDER BY st.id
          LIMIT 1
         RETURNING uuid",
    )
    .bind(env.session_id)
    .fetch_one(env.handle.pool())
    .await?)
}

async fn evaluation(
    env: &Env,
    turn: &Turn,
    label: &str,
    verdict_json: Value,
) -> anyhow::Result<Uuid> {
    let delivered = intervention(env).await?;
    Ok(sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO steering.evaluations
             (steerer_id, session_id, operator_turn_id, status, intervention_uuid, mode,
              trigger_point, trigger_request_id, resolved_at, outcome_label, label_source,
              labelled_at, label_evidence, verdict_json)
         SELECT st.id, $1, $2, 'resolved_correction', $3, 'live', 'turn_end', $4, now(), $5,
                'session_reply', now(), '{}'::jsonb, $6
           FROM steering.steerers st
          WHERE st.kind = 'verifier'
          ORDER BY st.id
          LIMIT 1
         RETURNING uuid",
    )
    .bind(env.session_id)
    .bind(turn.id)
    .bind(delivered)
    .bind(turn.start_request_id)
    .bind(label)
    .bind(verdict_json)
    .fetch_one(env.handle.pool())
    .await?)
}

fn prod_state(message: &str, chars: usize) -> Value {
    json!({"message": message, "message_chars": chars})
}

fn nudge_answer(noul: f64) -> Option<Value> {
    Some(json!({"only_nudge": {"type": "noul", "noul": noul}}))
}

const LONG: &str = "please rework the parser so that every error carries its position";

async fn prod_record(env: &Env, message: &str, chars: usize, noul: f64) -> anyhow::Result<i64> {
    let mut raw = Raw::new("prod_detect");
    raw.state = prod_state(message, chars);
    raw.answers = nudge_answer(noul);
    insert(env, raw).await
}

#[tokio::test]
async fn prod_detect_is_labelled_only_where_the_message_text_settles_it() -> anyhow::Result<()> {
    let Some(env) = Env::boot(1).await? else {
        return Ok(());
    };
    let turn = env.turns().await?;
    env.set_outcome(turn[0].id, "accepted", "operator").await?;
    let nudge_right = prod_record(&env, "  Continue.\n", 10, 0.9).await?;
    let nudge_wrong = prod_record(&env, "status?", 7, 0.1).await?;
    let content_right = prod_record(&env, LONG, LONG.len(), 0.1).await?;
    let content_wrong = prod_record(&env, LONG, LONG.len(), 0.9).await?;
    let middle = prod_record(&env, "fix the parser please", 21, 0.9).await?;
    let mut shadow = Raw::new("prod_detect");
    shadow.mode = "shadow";
    shadow.state = prod_state("go ahead", 8);
    shadow.answers = nudge_answer(0.8);
    let shadow = insert(&env, shadow).await?;
    let mut chosen = Raw::new("prod_detect");
    chosen.applied = Some("chosen");
    chosen.state = prod_state("fix the parser please", 21);
    chosen.answers = nudge_answer(0.1);
    chosen.operator_turn_id = Some(turn[0].id);
    let chosen = insert(&env, chosen).await?;
    assert_eq!(env.sync().await?, 5);
    let source = "message_anchor";
    assert_eq!(env.labels(nudge_right).await?, vec![pair("right", source)]);
    assert_eq!(env.labels(nudge_wrong).await?, vec![pair("wrong", source)]);
    assert_eq!(
        env.labels(content_right).await?,
        vec![pair("right", source)]
    );
    assert_eq!(
        env.labels(content_wrong).await?,
        vec![pair("wrong", source)]
    );
    assert_eq!(env.labels(shadow).await?, vec![pair("right", source)]);
    assert!(env.labels(middle).await?.is_empty());
    assert!(env.labels(chosen).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

async fn precheck_record(
    env: &Env,
    point: &str,
    turn: &Turn,
    send: bool,
    fire_label: &str,
) -> anyhow::Result<i64> {
    let uuid = evaluation(env, turn, fire_label, json!({"fires": true})).await?;
    let mut raw = Raw::new(point);
    raw.composed = Some(json!({"verdict": {"send": send, "score": 0.9}}));
    raw.evaluation_uuid = Some(uuid);
    insert(env, raw).await
}

#[tokio::test]
async fn precheck_is_right_when_it_sent_a_right_nudge_or_held_a_wrong_one() -> anyhow::Result<()> {
    let Some(env) = Env::boot(1).await? else {
        return Ok(());
    };
    let turns = env.turns().await?;
    let turn = &turns[0];
    let sent_right = precheck_record(&env, "verifier_precheck", turn, true, "right").await?;
    let sent_wrong = precheck_record(&env, "verifier_precheck", turn, true, "wrong").await?;
    let held_wrong = precheck_record(&env, "verifier_precheck", turn, false, "wrong").await?;
    let held_right = precheck_record(&env, "verifier_precheck", turn, false, "right").await?;
    let guard = precheck_record(&env, "turn_guard_precheck", turn, true, "right").await?;
    let open = precheck_record(&env, "verifier_precheck", turn, true, "indeterminate").await?;
    let source = "precheck_decision";
    assert_eq!(env.sync().await?, 5);
    assert_eq!(env.labels(sent_right).await?, vec![pair("right", source)]);
    assert_eq!(env.labels(sent_wrong).await?, vec![pair("wrong", source)]);
    assert_eq!(env.labels(held_wrong).await?, vec![pair("right", source)]);
    assert_eq!(env.labels(held_right).await?, vec![pair("wrong", source)]);
    assert_eq!(env.labels(guard).await?, vec![pair("right", source)]);
    assert!(
        env.labels(open)
            .await?
            .iter()
            .all(|(_, found)| found != source)
    );
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

async fn checklist_record(
    env: &Env,
    turn: &Turn,
    mode: &str,
    jev_fires: bool,
    nudge_label: &str,
) -> anyhow::Result<i64> {
    evaluation(env, turn, nudge_label, json!({"fires": true})).await?;
    let mut raw = Raw::new("steerer_checklist");
    raw.mode = mode;
    raw.composed = Some(json!({"verdict": {"fires": jev_fires}}));
    raw.operator_turn_id = Some(turn.id);
    raw.request_id = Some(turn.start_request_id);
    insert(env, raw).await
}

#[tokio::test]
async fn checklist_fire_is_scored_against_the_reply_to_the_nudge_it_sent() -> anyhow::Result<()> {
    let Some(env) = Env::boot(5).await? else {
        return Ok(());
    };
    let turns = env.turns().await?;
    let fired_right = checklist_record(&env, &turns[0], "on", true, "right").await?;
    let fired_wrong = checklist_record(&env, &turns[1], "shadow", true, "wrong").await?;
    let held_right = checklist_record(&env, &turns[2], "on", false, "right").await?;
    let held_wrong = checklist_record(&env, &turns[3], "on", false, "wrong").await?;
    let open = checklist_record(&env, &turns[4], "on", true, "indeterminate").await?;
    let source = "steerer_nudge_reply";
    assert_eq!(env.sync().await?, 4);
    assert_eq!(env.labels(fired_right).await?, vec![pair("right", source)]);
    assert_eq!(env.labels(fired_wrong).await?, vec![pair("wrong", source)]);
    assert_eq!(env.labels(held_right).await?, vec![pair("wrong", source)]);
    assert_eq!(env.labels(held_wrong).await?, vec![pair("right", source)]);
    assert!(env.labels(open).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

async fn gold(env: &Env, turn: &Turn, axis: &str, label: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO eval.turn_splits (operator_turn_id, split, split_reason)
         VALUES ($1, 'dev', 'hash') ON CONFLICT (operator_turn_id) DO NOTHING",
    )
    .bind(turn.id)
    .execute(env.handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO eval.labels
             (subject_kind, subject_id, operator_turn_id, session_id, session_uuid, axis, label,
              tier, method, split, source)
         SELECT 'turn', $1, $1, s.id, s.session_uuid, $2, $3, 'gold', 'operator_click', 'dev', 'spa'
           FROM sessions.sessions s
          WHERE s.id = $4",
    )
    .bind(turn.id)
    .bind(axis)
    .bind(label)
    .bind(env.session_id)
    .execute(env.handle.pool())
    .await?;
    Ok(())
}

async fn judge_record(
    env: &Env,
    turn: &Turn,
    reaction: &str,
    delivery: &str,
) -> anyhow::Result<i64> {
    let mut raw = Raw::new("turn_judge");
    raw.applied = Some("sent");
    raw.composed = Some(json!({"verdict": {
        "label": "accepted", "reaction": reaction, "delivery": delivery, "score": 0.9
    }}));
    raw.operator_turn_id = Some(turn.id);
    insert(env, raw).await
}

#[tokio::test]
async fn turn_judge_is_right_only_when_every_gold_axis_matches() -> anyhow::Result<()> {
    let Some(env) = Env::boot(6).await? else {
        return Ok(());
    };
    let turns = env.turns().await?;
    gold(&env, &turns[0], "reaction", "corrected").await?;
    gold(&env, &turns[0], "delivery", "partial").await?;
    gold(&env, &turns[1], "reaction", "corrected").await?;
    gold(&env, &turns[1], "delivery", "partial").await?;
    gold(&env, &turns[2], "reaction", "accepted").await?;
    gold(&env, &turns[3], "delivery", "shipped").await?;
    gold(&env, &turns[3], "reaction", "accepted").await?;
    gold(&env, &turns[4], "delivery", "not_applicable").await?;
    let both = judge_record(&env, &turns[0], "corrected", "partial").await?;
    let one_off = judge_record(&env, &turns[1], "corrected", "delivered").await?;
    let reaction_only = judge_record(&env, &turns[2], "accepted", "none").await?;
    let reaction_off = judge_record(&env, &turns[3], "corrected", "shipped").await?;
    let unscorable = judge_record(&env, &turns[4], "accepted", "delivered").await?;
    let ungolden = judge_record(&env, &turns[5], "accepted", "delivered").await?;
    let source = "operator_gold_axes";
    assert_eq!(env.sync().await?, 4);
    assert_eq!(env.labels(both).await?, vec![pair("right", source)]);
    assert_eq!(env.labels(one_off).await?, vec![pair("wrong", source)]);
    assert_eq!(
        env.labels(reaction_only).await?,
        vec![pair("right", source)]
    );
    assert_eq!(env.labels(reaction_off).await?, vec![pair("wrong", source)]);
    assert!(env.labels(unscorable).await?.is_empty());
    assert!(env.labels(ungolden).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}
