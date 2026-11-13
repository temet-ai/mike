WITH stamped AS (
    UPDATE memory.operator_turns t
       SET outcome_id = tot.id, classified_at = NOW(), judge_model = $7::text,
           judge_rubric_version = $4::text, judge_failure_reason = NULL,
           judge_attempt_count = 0, judge_next_attempt_at = NULL,
           rationale = $12::text, unmet_ask = $13::text, evidence_quality = $14::text,
           outcome_bundle_id = $3::bigint
      FROM memory.turn_outcomes tot, memory.turn_outcomes unk
     WHERE tot.code = $25::text AND t.id = $1::bigint
       AND unk.code = 'unknown' AND t.outcome_id = unk.id
 RETURNING t.id)
INSERT INTO memory.turn_verdicts (
    operator_turn_id, session_id, mode, bundle_id, rubric_version,
    input_builder_version, slices, model, source, reaction, delivery, legacy_label, confidence,
    rationale, unmet_ask, evidence_quality, projected, next_genuine_request_id, as_of,
    input_text, generation_id, cost_usd, failure_tags, severity, prompt_tokens,
    completion_tokens, latency_ms)
SELECT t.id, t.session_id, $2::text, $3::bigint, $4::text, $5::text, $6::text[], $7::text, 'llm',
       $8::text, $9::text, $10::text, $11::real, $12::text, $13::text, $14::text,
       EXISTS (SELECT 1 FROM stamped), $15::bigint, COALESCE(t.closed_at, $16::timestamptz),
       $17::text, $18::text, $19::float8, $20::text[], $21::text, $22::int, $23::int, $24::int
  FROM memory.operator_turns t
 WHERE t.id = $1::bigint
RETURNING id AS "id!", projected AS "projected!"
