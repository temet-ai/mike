use crate::store::persistence::launch_repo::LaunchPersist as _;
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
        .launch()
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
    sqlx::query(
        "UPDATE work.thread_state SET updated_at = now() - interval '2 days' WHERE thread_id = $1",
    )
    .bind(thread_id)
    .execute(pool)
    .await?;
    let (session_id, turns) = seeded_turns(pool, session, f.project).await?;
    seed_promises(pool, session_id, &turns).await?;
    seed_verdicts(pool, session_id, &turns).await?;

    let inputs = f
        .handle
        .launch()
        .work_thread_card_inputs(&[thread_id])
        .await?;
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
            .launch()
            .work_thread_card_inputs(&[i64::MAX])
            .await?
            .is_empty()
    );
    Ok(())
}

async fn lesson_ref(
    pool: &sqlx::PgPool,
    project: i64,
    text: &str,
    expires: &str,
) -> anyhow::Result<(i64, String)> {
    let (id, uuid): (i64, Uuid) = sqlx::query_as(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, topic, expires_at)
         SELECT $1, lk.id, $2, ls.id, lsrc.id, 'related', now() + ($3::text)::interval
           FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
          WHERE lk.code = 'correction' AND ls.code = 'daily' AND lsrc.code = 'operator'
      RETURNING id, uuid",
    )
    .bind(project)
    .bind(text)
    .bind(expires)
    .fetch_one(pool)
    .await?;
    Ok((id, format!("lesson:{uuid}")))
}

#[tokio::test]
async fn card_inputs_for_candidates_leave_out_expired_superseded_missing_and_weak_items()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let thread_id = f
        .handle
        .launch()
        .checkpoint_work_thread(f.project, &titled("related-inputs", "related inputs"))
        .await?
        .thread
        .id;
    let pool = f.handle.pool();
    let (_, live) = lesson_ref(pool, f.project, "Live lesson\nsecond line", "1 day").await?;
    let (_, expired) = lesson_ref(pool, f.project, "Expired lesson", "-1 day").await?;
    let (old_id, superseded) = lesson_ref(pool, f.project, "Superseded lesson", "1 day").await?;
    let (new_id, _) = lesson_ref(pool, f.project, "Replacement lesson", "1 day").await?;
    let (_, weak) = lesson_ref(pool, f.project, "Weakly related lesson", "1 day").await?;
    let other = f
        .handle
        .launch()
        .checkpoint_work_thread(f.project, &titled("older-effort", "Older effort"))
        .await?
        .thread
        .id;
    let imported: i64 = sqlx::query_scalar(
        "INSERT INTO work.thread_log (thread_id, ord, summary, kind)
         VALUES ($1, 99, 'Imported worklog /work/artifacts/threads/older-effort/worklog.md', 'note')
         RETURNING id",
    )
    .bind(other)
    .fetch_one(pool)
    .await?;
    let imported = format!("entry:{imported}");
    sqlx::query(
        "INSERT INTO memory.lesson_relations
            (from_lesson_id, to_lesson_id, kind_id, method_id, status_id,
             evidence_json, detector_version, decided_at, decided_by)
         SELECT $1, $2, rk.id, rm.id, rs.id, '{}'::jsonb, 'test', NOW(), 'test'
           FROM memory.lesson_relation_kinds rk,
                memory.lesson_relation_methods rm,
                memory.lesson_relation_statuses rs
          WHERE rk.code = 'supersedes' AND rm.code = 'declared' AND rs.code = 'confirmed'",
    )
    .bind(new_id)
    .bind(old_id)
    .execute(pool)
    .await?;
    let candidates: Vec<_> = [
        (&expired, 0.95),
        (&superseded, 0.9),
        (&"entry:999999999".to_owned(), 0.85),
        (&live, 0.5),
        (&imported, 0.4),
        (&weak, -2.0),
    ]
    .into_iter()
    .map(
        |(reference, score)| crate::store::persistence::work_threads::NewThreadCandidate {
            kind: reference.split(':').next().unwrap_or_default().to_owned(),
            reference: reference.clone(),
            score,
        },
    )
    .collect();
    f.handle
        .launch()
        .replace_thread_card_candidates(thread_id, &candidates)
        .await?;

    let inputs = f
        .handle
        .launch()
        .work_thread_card_inputs(&[thread_id])
        .await?;

    let [card] = inputs.as_slice() else {
        anyhow::bail!("expected one thread's inputs, got {}", inputs.len());
    };
    let related: Vec<(&str, &str)> = card
        .related
        .iter()
        .map(|item| (item.reference.as_str(), item.text.as_str()))
        .collect();
    assert_eq!(
        related,
        vec![
            (live.as_str(), "Live lesson"),
            (imported.as_str(), "thread older-effort: A thread"),
        ]
    );
    Ok(())
}
