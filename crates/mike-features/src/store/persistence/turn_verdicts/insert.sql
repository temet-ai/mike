WITH ins AS (
    INSERT INTO memory.turn_verdicts (
        operator_turn_id, session_id, mode, bundle_id, rubric_version,
        input_builder_version, slices, model, source, reaction, delivery, legacy_label, confidence,
        rationale, unmet_ask, evidence_quality, projected, next_genuine_request_id, as_of,
        input_text, generation_id, cost_usd, failure_tags, severity, prompt_tokens,
        completion_tokens, latency_ms)
    SELECT t.id, t.session_id, $2::text, $3::bigint, $4::text, $5::text, $6::text[], $7::text, 'llm',
           $8::text, $9::text, $10::text, $11::real, $12::text, $13::text, $14::text,
           $25::boolean, $15::bigint, $16::timestamptz, $17::text, $18::text,
           $19::float8, $20::text[], $21::text, $22::int, $23::int, $24::int
      FROM memory.operator_turns t
     WHERE t.id = $1::bigint
    RETURNING id, operator_turn_id, mode, reaction, next_genuine_request_id),
dequeued AS (
    UPDATE memory.operator_turns t
       SET judge_queued_at = NULL
      FROM ins
      JOIN memory.turn_facts f ON f.operator_turn_id = ins.operator_turn_id
     WHERE t.id = ins.operator_turn_id
       AND t.judge_queued_at IS NOT NULL
       AND ins.mode = 'live'
       AND ins.reaction IS NOT NULL
       AND ins.next_genuine_request_id IS NOT DISTINCT FROM f.next_genuine_request_id)
SELECT id AS "id!" FROM ins
