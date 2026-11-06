use serde_json::json;

use crate::turn_facts::{Fixture, NewTurn, Req, fixture, insert_request, insert_turn};

async fn guard_id(f: &Fixture) -> anyhow::Result<i64> {
    Ok(f.handle
        .guard_steerer()
        .await?
        .ok_or_else(|| anyhow::anyhow!("turn-guard row not seeded"))?
        .id)
}

async fn resolved_turn(f: &Fixture, start: i64) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(start, 0, "run the tests")).await?;
    let turn = insert_turn(f, &NewTurn::closed(start, start, 0, 30)).await?;
    let mut row = crate::turn_facts::sample_row(turn, f.session_id, chrono::Utc::now());
    crate::turn_facts::resolve(&mut row);
    f.handle.upsert_turn_facts(&[row]).await?;
    Ok(turn)
}

struct EvalSeed<'a> {
    mode: &'a str,
    status: &'a str,
    arm: Option<&'a str>,
    question: &'a str,
}

async fn evaluation(
    f: &Fixture,
    steerer: i64,
    turn: i64,
    seed: EvalSeed<'_>,
) -> anyhow::Result<uuid::Uuid> {
    let holdback_draw = seed.arm.map(|_| 0.5_f64);
    let mut gates = serde_json::Map::new();
    gates.insert(seed.question.to_owned(), json!({"open": true}));
    let verdict = json!({"gates": gates, "actor": {"agent_instance_id": null}});
    Ok(sqlx::query_scalar(
        "INSERT INTO steering.evaluations
            (steerer_id, session_id, operator_turn_id, status, mode, arm, holdback_draw,
             arm_anchor_turn_id, question, trigger_point, dispatched_at, resolved_at, verdict_json)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $3, $8, 'turn_end', now(), now(), $9)
         RETURNING uuid",
    )
    .bind(steerer)
    .bind(f.session_id)
    .bind(turn)
    .bind(seed.status)
    .bind(seed.mode)
    .bind(seed.arm)
    .bind(holdback_draw)
    .bind(seed.question)
    .bind(verdict)
    .fetch_one(f.handle.pool())
    .await?)
}

#[tokio::test]
async fn label_candidates_admit_live_pass_rows_only_when_setting_is_on() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let id = guard_id(&f).await?;
    let turn = resolved_turn(&f, 60_001).await?;
    let pass = evaluation(
        &f,
        id,
        turn,
        EvalSeed {
            mode: "live",
            status: "resolved_pass",
            arm: None,
            question: "relapse",
        },
    )
    .await?;
    let fired = evaluation(
        &f,
        id,
        turn,
        EvalSeed {
            mode: "live",
            status: "resolved_correction",
            arm: None,
            question: "unbacked_wait",
        },
    )
    .await?;
    let treated = evaluation(
        &f,
        id,
        turn,
        EvalSeed {
            mode: "live",
            status: "resolved_pass",
            arm: Some("treated"),
            question: "unobserved_done",
        },
    )
    .await?;

    let off = f.handle.guard_label_candidates(id, 200, false).await?;
    let off_ids: Vec<uuid::Uuid> = off.iter().map(|c| c.uuid).collect();
    assert!(
        !off_ids.contains(&pass) && !off_ids.contains(&fired) && !off_ids.contains(&treated),
        "label_live_passes off admits no live row: {off_ids:?}"
    );

    let on = f.handle.guard_label_candidates(id, 200, true).await?;
    let on_ids: Vec<uuid::Uuid> = on.iter().map(|c| c.uuid).collect();
    assert!(on_ids.contains(&pass), "an untreated live pass is admitted");
    assert!(
        !on_ids.contains(&fired),
        "a fire (resolved_correction) is never labelled as a pass"
    );
    assert!(
        !on_ids.contains(&treated),
        "a treated fire stays excluded: its next message is confounded by the nudge"
    );
    Ok(())
}
