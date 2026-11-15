use serde_json::{Value, json};

use super::outcome_sync_support_tests::{Env, NewRecord, pair};

const SEARCH_SOURCE: &str = "search_result_used";
const PRELABEL_SOURCE: &str = "operator_gold_label";

fn noul(p: f64) -> Value {
    json!({"type": "noul", "noul": p})
}

fn grade_record(event_id: Option<i64>, answers: Value) -> NewRecord<'static> {
    let mut verdict = json!({"score": 0.8});
    if let (Some(id), Some(fields)) = (event_id, verdict.as_object_mut()) {
        fields.insert("search_event_id".to_owned(), json!(id));
    }
    let mut record = NewRecord::new("search_grade", "labelled");
    record.composed = json!({"error": null, "answers": answers, "verdict": verdict});
    record
}

async fn search_event(
    env: &Env,
    query: &str,
    references: &[&str],
    used: &[&str],
    final_used: bool,
) -> anyhow::Result<i64> {
    let returned: Vec<Value> = references
        .iter()
        .map(|r| json!({"reference": r, "score": 0.5}))
        .collect();
    let used: Vec<String> = used.iter().map(|u| (*u).to_owned()).collect();
    Ok(sqlx::query_scalar::<_, i64>(
        "INSERT INTO memory.search_events
             (session_id, tool, query, returned, rerank_state, used_references, used_final_at)
         VALUES ($1, 'search_project_knowledge', $2, $3, 'reranked', $4,
                 CASE WHEN $5 THEN now() ELSE NULL END)
         RETURNING id",
    )
    .bind(env.session_id)
    .bind(query)
    .bind(Value::Array(returned))
    .bind(used)
    .bind(final_used)
    .fetch_one(env.handle.pool())
    .await?)
}

#[tokio::test]
async fn search_grade_is_right_when_a_relevant_result_was_cited_and_wrong_when_an_irrelevant_one_was()
-> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    let tag = uuid::Uuid::new_v4();
    let cited_relevant =
        search_event(&env, &format!("a {tag}"), &["a1", "a2"], &["a1"], true).await?;
    let cited_irrelevant =
        search_event(&env, &format!("b {tag}"), &["b1", "b2"], &["b2"], true).await?;
    let still_open = search_event(&env, &format!("c {tag}"), &["c1"], &["c1"], false).await?;
    let answers = json!({"result_1": noul(0.9), "result_2": noul(0.1)});
    let wrong_answers = json!({"result_1": noul(0.9), "result_2": noul(0.2)});
    let right = env
        .record(grade_record(Some(cited_relevant), answers))
        .await?;
    let wrong = env
        .record(grade_record(Some(cited_irrelevant), wrong_answers))
        .await?;
    let open = env
        .record(grade_record(
            Some(still_open),
            json!({"result_1": noul(0.9)}),
        ))
        .await?;
    assert_eq!(env.sync().await?, 2);
    assert_eq!(env.labels(right).await?, vec![pair("right", SEARCH_SOURCE)]);
    assert_eq!(env.labels(wrong).await?, vec![pair("wrong", SEARCH_SOURCE)]);
    assert!(env.labels(open).await?.is_empty());
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

#[tokio::test]
async fn search_grade_without_an_event_id_finds_the_event_by_its_query() -> anyhow::Result<()> {
    let Some(env) = Env::boot(0).await? else {
        return Ok(());
    };
    let query = format!("unused query {}", uuid::Uuid::new_v4());
    search_event(&env, &query, &["x1", "x2"], &[], true).await?;
    let record = env
        .record(grade_record(
            None,
            json!({"result_1": noul(0.1), "result_2": noul(0.2)}),
        ))
        .await?;
    sqlx::query("UPDATE decisions.records SET state = $2 WHERE id = $1")
        .bind(record)
        .bind(json!({"query": query}))
        .execute(env.handle.pool())
        .await?;
    assert_eq!(env.sync().await?, 1);
    assert_eq!(
        env.labels(record).await?,
        vec![pair("right", SEARCH_SOURCE)]
    );
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}

async fn human_reactions(env: &Env, reactions: &[(i64, &str)]) -> anyhow::Result<()> {
    for (turn_id, label) in reactions {
        sqlx::query(
            "INSERT INTO eval.turn_splits (operator_turn_id, split, split_reason)
             VALUES ($1, 'dev', 'hash') ON CONFLICT DO NOTHING",
        )
        .bind(*turn_id)
        .execute(env.handle.pool())
        .await?;
        sqlx::query(
            "INSERT INTO eval.labels (subject_kind, subject_id, operator_turn_id, session_id,
                                      session_uuid, axis, label, tier, method, split, source)
             VALUES ('turn', $1, $1, $2, $3, 'reaction', $4, 'gold', 'operator_click', 'dev',
                     'spa')",
        )
        .bind(*turn_id)
        .bind(env.session_id)
        .bind(env.session_uuid)
        .bind(*label)
        .execute(env.handle.pool())
        .await?;
    }
    Ok(())
}

async fn prelabel_record(env: &Env, turn_id: i64, choice: &str) -> anyhow::Result<i64> {
    let mut record = NewRecord::new("label_prelabel", "labelled");
    record.composed = json!({
        "answers": {
            format!("turn_{turn_id}"): {
                "type": "choice", "choice": choice, "confidence": 0.7,
                "probabilities": {choice: 0.7}
            }
        },
        "verdict": {"score": 0.7}
    });
    env.record(record).await
}

#[tokio::test]
async fn label_prelabel_is_scored_against_the_operators_newest_reaction_label() -> anyhow::Result<()>
{
    let Some(env) = Env::boot(3).await? else {
        return Ok(());
    };
    let turns = env.turns().await?;
    human_reactions(
        &env,
        &[
            (turns[0].id, "accepted"),
            (turns[1].id, "accepted"),
            (turns[1].id, "corrected"),
            (turns[2].id, "redirected"),
        ],
    )
    .await?;
    let right = prelabel_record(&env, turns[0].id, "accepted").await?;
    let wrong = prelabel_record(&env, turns[1].id, "accepted").await?;
    let written_at_call_time = prelabel_record(&env, turns[2].id, "redirected").await?;
    sqlx::query(
        "INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
         VALUES ($1, 'right', $2, '{}'::jsonb)",
    )
    .bind(written_at_call_time)
    .bind(PRELABEL_SOURCE)
    .execute(env.handle.pool())
    .await?;
    assert_eq!(env.sync().await?, 2);
    assert_eq!(
        env.labels(right).await?,
        vec![pair("right", PRELABEL_SOURCE)]
    );
    assert_eq!(
        env.labels(wrong).await?,
        vec![pair("wrong", PRELABEL_SOURCE)]
    );
    assert_eq!(
        env.labels(written_at_call_time).await?,
        vec![pair("right", PRELABEL_SOURCE)]
    );
    assert_eq!(env.sync().await?, 0);
    env.finish().await
}
