SELECT v.id, v.session_id, o.code AS outcome, v.quality_score::float8 AS "quality_score?",
       v.rationale, v.judge_model, v.rubric_version, v.judged_at, v.mode, v.bundle_id, v.rubric_id,
       v.end_turn_id, v.end_label, v.end_source, v.end_rule, v.end_confidence, v.end_evidence,
       v.failure, v.failure_tags, v.severity, v.slices, v.as_of, v.digest_version, v.input_text,
       v.llm_output, v.generation_id, v.cost_usd, v.prompt_tokens, v.completion_tokens, v.latency_ms,
       count(*) OVER () AS "total!"
  FROM session_judges.verdicts v
  JOIN session_judges.outcomes o ON o.id = v.outcome_id
 WHERE v.session_id = $1
 ORDER BY v.judged_at DESC, v.id DESC
 LIMIT $2 OFFSET $3
