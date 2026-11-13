SELECT v.id AS "id!", v.operator_turn_id AS "operator_turn_id!", v.session_id AS "session_id!",
       s.session_uuid AS "session_uuid!", o.code AS "turn_outcome?", v.mode AS "mode!",
       v.bundle_id, v.rubric_version AS "rubric_version!",
       v.input_builder_version AS "input_builder_version!", v.slices AS "slices!", v.model AS "model!",
       v.source AS "source!", v.reaction, v.delivery, v.legacy_label,
       v.confidence AS "confidence!", v.rationale AS "rationale!", v.unmet_ask AS "unmet_ask!",
       v.evidence_quality AS "evidence_quality!", v.projected AS "projected!",
       v.next_genuine_request_id, v.as_of AS "as_of!", v.input_text AS "input_text!",
       v.generation_id, v.cost_usd, v.failure_tags, v.severity,
       v.prompt_tokens, v.completion_tokens, v.latency_ms, v.created_at AS "created_at!"
  FROM memory.turn_verdicts v
  JOIN sessions.sessions s ON s.id = v.session_id
  JOIN memory.operator_turns t ON t.id = v.operator_turn_id
  LEFT JOIN memory.turn_outcomes o ON o.id = t.outcome_id
 WHERE v.operator_turn_id = $1 AND v.mode = ANY($2::text[])
 ORDER BY v.created_at DESC, v.id DESC
 LIMIT $3 OFFSET $4
