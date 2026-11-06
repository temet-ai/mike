-- #151: 'permanent' now excludes a turn from claim_unjudged_turns forever (previously it was diagnostic-only) — rows written before that meaning existed are cleared here so they return to the queue, not corrected as bad data.

UPDATE memory.operator_turns
   SET judge_failure_reason = NULL,
       judge_attempt_count = 0,
       judge_next_attempt_at = NULL
 WHERE judge_failure_reason = 'permanent';
