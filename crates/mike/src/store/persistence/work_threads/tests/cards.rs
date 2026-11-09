use uuid::Uuid;

use super::super::CheckpointInput;
use super::{fixture, titled};
use crate::store::persistence::test_support::seed_session_turns;

const PROMISE_TEXT: &str = "Done with the render. I will run the sweep next.";

async fn seeded_turns(
    pool: &sqlx::PgPool,
    session: Uuid,
    project: i64,
) -> anyhow::Result<(i64, Vec<i64>)> {
    let session_id: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1, $2, now(), now()) RETURNING id",
    )
    .bind(session)
    .bind(project)
    .fetch_one(pool)
    .await?;
    seed_session_turns(pool, &session.to_string(), project, 3).await?;
    let turns: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM memory.operator_turns WHERE session_id = $1 ORDER BY id",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    anyhow::ensure!(turns.len() == 3, "three turns seeded");
    Ok((session_id, turns))
}

async fn seed_promises(pool: &sqlx::PgPool, session_id: i64, turns: &[i64]) -> anyhow::Result<()> {
    sqlx::query("UPDATE memory.operator_turns SET final_assistant_text = $2 WHERE id = $1")
        .bind(turns[0])
        .bind(PROMISE_TEXT)
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO memory.dossier_entries
             (session_id, operator_turn_id, kind, data, status, source, source_ref, dedup_key,
              extractor_version, effective_at)
         VALUES ($1, $2, 'promise', '{\"span\": [22, 48]}', 'open', 'capture',
                 'memory.operator_turns:' || $2, 'card-test:' || $2, 'test', now() - interval '1 day'),
                ($1, $3, 'promise', '{\"span\": \"bad\"}', 'open', 'capture',
                 'memory.operator_turns:' || $3, 'card-test:' || $3, 'test', now() - interval '20 days')",
    )
    .bind(session_id)
    .bind(turns[0])
    .bind(turns[1])
    .execute(pool)
    .await?;
    Ok(())
}

async fn seed_verdicts(pool: &sqlx::PgPool, session_id: i64, turns: &[i64]) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.turn_verdicts
             (operator_turn_id, session_id, mode, rubric_version, input_builder_version, model,
              reaction, delivery, confidence, rationale, unmet_ask, evidence_quality, as_of,
              input_text, created_at)
         SELECT v.turn, $1, 'live', 'test', 'test', 'test', 'unclear', 'none', 0.5, 'test',
                v.ask, 'asserted', now(), 'test', now() - v.age
           FROM (VALUES ($2::bigint, 'Show the card in the thread page', interval '1 day'),
                        ($3::bigint, 'An ask from three weeks ago', interval '21 days')) AS v(turn, ask, age)",
    )
    .bind(session_id)
    .bind(turns[1])
    .bind(turns[2])
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn card_inputs_for_recent_and_old_items_keep_only_the_recent_ones() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let session = Uuid::new_v4();
    let thread_id = f
        .handle
        .checkpoint_work_thread(
            f.project,
            &CheckpointInput {
                session_uuid: Some(session),
                ..titled("card-inputs", "card inputs")
            },
        )
        .await?
        .thread
        .id;
    let pool = f.handle.pool();
    let (session_id, turns) = seeded_turns(pool, session, f.project).await?;
    seed_promises(pool, session_id, &turns).await?;
    seed_verdicts(pool, session_id, &turns).await?;

    let inputs = f.handle.work_thread_card_inputs(&[thread_id]).await?;
    let [card] = inputs.as_slice() else {
        anyhow::bail!("expected one thread's inputs, got {}", inputs.len());
    };
    let promises: Vec<Option<&str>> = card.promises.iter().map(|p| p.text.as_deref()).collect();
    assert_eq!(promises, vec![Some("I will run the sweep next.")]);
    let asks: Vec<&str> = card
        .unmet_asks
        .iter()
        .map(|a| a.unmet_ask.as_str())
        .collect();
    assert_eq!(asks, vec!["Show the card in the thread page"]);
    Ok(())
}

#[tokio::test]
async fn card_inputs_for_unknown_thread_are_empty() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    assert!(
        f.handle
            .work_thread_card_inputs(&[i64::MAX])
            .await?
            .is_empty()
    );
    Ok(())
}
