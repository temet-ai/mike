mod api_tests;
mod placement_tests;
mod relate_flow_tests;
mod select_tests;
mod shape_pass_tests;
mod support;
mod write_tests;

use mike::store::persistence::lessons::proposals::CorrectionCandidate;

use crate::turn_facts::{
    Fixture, NewTurn, Req, attach, insert_request, insert_turn, resolve, sample_row,
};

pub(crate) async fn seed_corrected_turn(
    f: &Fixture,
    base_id: i64,
) -> anyhow::Result<CorrectionCandidate> {
    seed_corrected_turn_with_confidence(f, base_id, 0.9).await
}

pub(crate) async fn seed_corrected_turn_with_confidence(
    f: &Fixture,
    base_id: i64,
    confidence: f32,
) -> anyhow::Result<CorrectionCandidate> {
    insert_request(f, &Req::primary(base_id, 0, "please run the tests")).await?;
    let turn = insert_turn(f, &NewTurn::closed(base_id, base_id, 0, 30)).await?;
    attach(f, turn, &[base_id]).await?;
    insert_request(
        f,
        &Req::primary(
            base_id + 1,
            40,
            "no, you never actually ran the test suite, you just said it passed",
        ),
    )
    .await?;

    mark_operator_request(f, base_id + 1).await?;

    let mut facts = sample_row(turn, f.session_id, f.base);
    resolve(&mut facts);
    facts.next_genuine_request_id = Some(base_id + 1);
    f.handle
        .upsert_turn_facts(std::slice::from_ref(&facts))
        .await?;

    sqlx::query(
        "UPDATE memory.operator_turns SET final_assistant_text = $2, user_prompt = $3 WHERE id = $1",
    )
    .bind(turn)
    .bind("done, tests pass")
    .bind("please run the tests")
    .execute(f.handle.pool())
    .await?;

    let verdict_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.turn_verdicts
            (operator_turn_id, session_id, mode, rubric_version, input_builder_version, model,
             reaction, delivery, confidence, rationale, unmet_ask, evidence_quality, as_of, input_text)
         VALUES ($1, $2, 'live', 'v2', 'judge-input-v2.1', 'm',
                 'corrected', 'delivered', $3, 'skipped a step', 'run the tests', 'verified',
                 now(), 'input')
         RETURNING id",
    )
    .bind(turn)
    .bind(f.session_id)
    .bind(confidence)
    .fetch_one(f.handle.pool())
    .await?;

    sqlx::query(
        "UPDATE memory.turn_verdicts SET projected=true,next_genuine_request_id=$2 WHERE id=$1",
    )
    .bind(verdict_id)
    .bind(base_id + 1)
    .execute(f.handle.pool())
    .await?;
    sqlx::query("UPDATE memory.operator_turns SET live_verdict_id=$2 WHERE id=$1")
        .bind(turn)
        .bind(verdict_id)
        .execute(f.handle.pool())
        .await?;
    Ok(CorrectionCandidate {
        verdict_id,
        operator_turn_id: turn,
        session_id: f.session_id,
        project_id: f.project_id,
        confidence,
        rationale: "skipped a step".to_owned(),
        unmet_ask: "run the tests".to_owned(),
        user_prompt: Some("please run the tests".to_owned()),
        final_assistant_text: Some("done, tests pass".to_owned()),
        correcting_message: Some(
            "no, you never actually ran the test suite, you just said it passed".to_owned(),
        ),
    })
}

pub(crate) async fn mark_operator_request(f: &Fixture, request_id: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE requests.requests SET author_kind_id=(SELECT id FROM requests.author_kinds WHERE code='operator') WHERE id=$1")
        .bind(request_id).execute(f.handle.pool()).await?;
    sqlx::query("INSERT INTO prompt_kinds.classifications (request_id,session_id,kind_id) SELECT r.id,r.session_id,k.id FROM requests.requests r CROSS JOIN prompt_kinds.kinds k WHERE r.id=$1 AND k.code='operator'")
        .bind(request_id).execute(f.handle.pool()).await?;
    Ok(())
}
