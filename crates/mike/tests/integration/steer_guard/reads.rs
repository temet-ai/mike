use serde_json::json;

use crate::turn_facts::{Fixture, NewTurn, Req, fixture, insert_request, insert_turn};

async fn guard_id(f: &Fixture) -> anyhow::Result<i64> {
    Ok(f.handle
        .guard_steerer()
        .await?
        .ok_or_else(|| anyhow::anyhow!("turn-guard row not seeded"))?
        .id)
}

async fn session_uuid(f: &Fixture) -> anyhow::Result<uuid::Uuid> {
    Ok(
        sqlx::query_scalar("SELECT session_uuid FROM sessions.sessions WHERE id = $1")
            .bind(f.session_id)
            .fetch_one(f.handle.pool())
            .await?,
    )
}

#[tokio::test]
async fn reach_read_uses_session_or_local_uuid() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let uuid = session_uuid(&f).await?;
    sqlx::query(
        "INSERT INTO remote_control.channels (cse_id, local_session_uuid, stream_opened_at, last_seen_at, last_heartbeat_interval_seconds)
         VALUES ('cse_' || $1::text, $1, now() - interval '5 minutes', now() - interval '10 seconds', 30)",
    )
    .bind(uuid)
    .execute(f.handle.pool())
    .await?;
    let by_uuid = f.handle.channel_reach_rows(-1, Some(uuid)).await?;
    assert_eq!(by_uuid.len(), 1);
    let none = f.handle.channel_reach_rows(-1, None).await?;
    assert!(none.is_empty());
    Ok(())
}

#[tokio::test]
async fn daily_cap_counts_guard_calls_today_only() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let id = guard_id(&f).await?;
    sqlx::query(
        "INSERT INTO llm.calls (role, model, outcome, steerer_id, created_at) VALUES
            ('steerer', 'm', 'ok', $1, now()),
            ('steerer', 'm', 'ok', $1, now()),
            ('steerer', 'm', 'ok', $1, now() - interval '2 days'),
            ('eval', 'm', 'ok', $1, now()),
            ('steerer', 'm', 'ok', $1 + 1000, now())",
    )
    .bind(id)
    .execute(f.handle.pool())
    .await?;
    assert_eq!(f.handle.steerer_calls_today(id, "steerer").await?, 2);
    assert_eq!(f.handle.steerer_calls_today(id, "eval").await?, 1);
    Ok(())
}

async fn evaluation(
    f: &Fixture,
    steerer: i64,
    turn: i64,
    mode: &str,
    question: &str,
    minutes_ago: i64,
) -> anyhow::Result<uuid::Uuid> {
    let mut gates = serde_json::Map::new();
    gates.insert(question.to_owned(), json!({"open": true}));
    let verdict = json!({"gates": gates, "actor": {"agent_instance_id": null}});
    Ok(sqlx::query_scalar(
        "INSERT INTO steering.evaluations (steerer_id, session_id, operator_turn_id, status, mode, question,
                                           trigger_point, dispatched_at, resolved_at, verdict_json)
         VALUES ($1, $2, $3, 'resolved_correction', $4, $5, 'turn_end', now() - make_interval(mins => $6::int),
                 now(), $7)
         RETURNING uuid",
    )
    .bind(steerer)
    .bind(f.session_id)
    .bind(turn)
    .bind(mode)
    .bind(question)
    .bind(minutes_ago)
    .bind(verdict)
    .fetch_one(f.handle.pool())
    .await?)
}

async fn resolved_turn(f: &Fixture, start: i64) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(start, 0, "run the tests")).await?;
    let turn = insert_turn(f, &NewTurn::closed(start, start, 0, 30)).await?;
    let mut row = crate::turn_facts::sample_row(turn, f.session_id, chrono::Utc::now());
    crate::turn_facts::resolve(&mut row);
    f.handle.upsert_turn_facts(&[row]).await?;
    Ok(turn)
}

#[tokio::test]
async fn label_candidates_exclude_live_rows_and_pick_last_row_per_turn_question()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let id = guard_id(&f).await?;
    let turn = resolved_turn(&f, 40_001).await?;
    let older = evaluation(&f, id, turn, "shadow", "relapse", 10).await?;
    let newer = evaluation(&f, id, turn, "shadow", "relapse", 1).await?;
    let live = evaluation(&f, id, turn, "live", "unobserved_done", 1).await?;
    let candidates = f.handle.guard_label_candidates(id, 200, false).await?;
    let ids: Vec<uuid::Uuid> = candidates.iter().map(|c| c.uuid).collect();
    assert!(
        !ids.contains(&live),
        "live rows are never labelled automatically"
    );
    let last: Vec<(uuid::Uuid, bool)> = candidates.iter().map(|c| (c.uuid, c.is_last)).collect();
    assert!(last.contains(&(newer, true)));
    assert!(last.contains(&(older, false)));
    Ok(())
}

#[tokio::test]
async fn audit_set_includes_launch_seats_and_role_matches_excludes_coordinators()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let uuid = session_uuid(&f).await?;
    sqlx::query(
        "INSERT INTO launch.launches (session_uuid, backend, folder, seat) VALUES ($1, 'herdr', '/work', 'w3')",
    )
    .bind(uuid)
    .execute(f.handle.pool())
    .await?;
    let audit = f
        .handle
        .guard_audit_sessions(chrono::Utc::now() - chrono::Duration::days(14))
        .await?;
    assert!(audit.contains(&f.session_id));
    sqlx::query("DELETE FROM launch.launches WHERE session_uuid = $1")
        .bind(uuid)
        .execute(f.handle.pool())
        .await?;
    sqlx::query(
        "INSERT INTO sessions.session_goals (session_id, goal, role) VALUES ($1, 'coordinate', 'fleet-coordinator')
         ON CONFLICT (session_id) DO UPDATE SET role = EXCLUDED.role",
    )
    .bind(f.session_id)
    .execute(f.handle.pool())
    .await?;
    let audit = f
        .handle
        .guard_audit_sessions(chrono::Utc::now() - chrono::Duration::days(14))
        .await?;
    assert!(
        !audit.contains(&f.session_id),
        "a coordinator is not in the audit set"
    );
    Ok(())
}
