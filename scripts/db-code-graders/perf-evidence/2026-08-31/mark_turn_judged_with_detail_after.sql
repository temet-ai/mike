UPDATE memory.operator_turns t
   SET outcome_id           = tot.id,
       classified_at        = NOW(),
       judge_model          = 'google/gemini-3-flash-preview',
       judge_rubric_version = 'r3-2026-08-29',
       judge_failure_reason  = NULL,
       judge_attempt_count   = 0,
       judge_next_attempt_at = NULL,
       rationale             = 'the operator asked for a fix and the turn showed the passing test run',
       unmet_ask             = '',
       evidence_quality      = 'verified'
  FROM memory.turn_outcomes tot
 WHERE tot.code = 'accepted'
   AND t.id     = 14462
   AND t.outcome_id = (SELECT id FROM memory.turn_outcomes WHERE code = 'unknown');
