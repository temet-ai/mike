use mike::lesson_proposer::ProposerSettings;
use mike::lesson_proposer::select::select_candidates;
use serde_json::json;

use crate::turn_facts::{
    Fixture, NewTurn, Req, attach, fixture, insert_request, insert_turn, resolve, sample_row,
};

async fn seed_verdict(
    f: &Fixture,
    base_id: i64,
    confidence: f32,
    resolved: bool,
) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(base_id, 0, "ask")).await?;
    let turn = insert_turn(f, &NewTurn::closed(base_id, base_id, 0, 30)).await?;
    attach(f, turn, &[base_id]).await?;

    let mut facts = sample_row(turn, f.session_id, f.base);
    if resolved {
        insert_request(f, &Req::primary(base_id + 1, 40, "no, that is not correct")).await?;
        super::mark_operator_request(f, base_id + 1).await?;
        resolve(&mut facts);
        facts.next_genuine_request_id = Some(base_id + 1);
    }
    f.handle
        .upsert_turn_facts(std::slice::from_ref(&facts))
        .await?;

    let verdict_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.turn_verdicts
            (operator_turn_id, session_id, mode, rubric_version, input_builder_version, model,
             reaction, delivery, confidence, rationale, unmet_ask, evidence_quality, as_of, input_text)
         VALUES ($1, $2, 'live', 'v2', 'judge-input-v2.1', 'm',
                 'corrected', 'delivered', $3, 'r', 'u', 'verified', now(), 'input')
         RETURNING id",
    )
    .bind(turn)
    .bind(f.session_id)
    .bind(confidence)
    .fetch_one(f.handle.pool())
    .await?;
    sqlx::query("UPDATE memory.turn_verdicts v SET projected = true, next_genuine_request_id = tf.next_genuine_request_id FROM memory.turn_facts tf WHERE v.id = $1 AND tf.operator_turn_id = v.operator_turn_id")
        .bind(verdict_id)
        .execute(f.handle.pool())
        .await?;
    sqlx::query("UPDATE memory.operator_turns SET live_verdict_id = $2 WHERE id = $1")
        .bind(turn)
        .bind(verdict_id)
        .execute(f.handle.pool())
        .await?;
    Ok(verdict_id)
}

#[tokio::test]
async fn select_candidates_includes_low_confidence_but_excludes_unresolved_and_already_examined()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let high_confidence = seed_verdict(&f, 41_100, 0.9, true).await?;
    let low_confidence = seed_verdict(&f, 41_110, 0.5, true).await?;
    seed_verdict(&f, 41_120, 0.9, false).await?;
    let already_examined = seed_verdict(&f, 41_130, 0.9, true).await?;
    sqlx::query(
        "INSERT INTO memory.lesson_proposal_evidence
            (verdict_id, operator_turn_id, similar_corrections, model)
         SELECT $1, v.operator_turn_id, $2, 'm'
           FROM memory.turn_verdicts v WHERE v.id = $1",
    )
    .bind(already_examined)
    .bind(json!([]))
    .execute(f.handle.pool())
    .await?;

    let settings = ProposerSettings {
        min_confidence: 0.7,
        max_per_day: 40,
        ..ProposerSettings::default()
    };
    let candidates = select_candidates(&f.handle, &settings).await?;
    let ids: Vec<i64> = candidates.iter().map(|c| c.verdict_id).collect();
    assert_eq!(ids, vec![high_confidence, low_confidence]);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn select_candidates_batch_is_not_capped_by_a_tiny_max_per_day() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let low_a = seed_verdict(&f, 41_200, 0.5, true).await?;
    let low_b = seed_verdict(&f, 41_210, 0.5, true).await?;
    let low_c = seed_verdict(&f, 41_220, 0.5, true).await?;
    let high = seed_verdict(&f, 41_230, 0.9, true).await?;

    let settings = ProposerSettings {
        min_confidence: 0.7,
        max_per_day: 1,
        ..ProposerSettings::default()
    };
    let candidates = select_candidates(&f.handle, &settings).await?;
    let ids: Vec<i64> = candidates.iter().map(|c| c.verdict_id).collect();
    assert_eq!(
        ids,
        vec![low_a, low_b, low_c, high],
        "a max_per_day of 1 governs the LLM-call budget in run_tick, not the fetch batch \
         size, so a run of low-confidence noise must not crowd a same-day high-confidence \
         correction out of the selected batch"
    );
    f.db.cleanup().await?;
    Ok(())
}
