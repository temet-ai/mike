UPDATE memory.operator_turns t
    SET judge_failure_reason  = 'transient_exhausted',
        judge_attempt_count   = t.judge_attempt_count + 1,
        judge_next_attempt_at = CASE
            WHEN 'transient_exhausted' = 'transient_exhausted'
            THEN now() + (INTERVAL '1 second' * LEAST(
                3600::float8,
                30::float8 * POWER(2::float8, t.judge_attempt_count::float8)
            ))
            ELSE NULL
        END
  WHERE t.id = 17777
    AND NOT EXISTS (
        SELECT 1 FROM memory.turn_verdicts v
          JOIN memory.turn_facts f ON f.operator_turn_id = v.operator_turn_id
         WHERE v.operator_turn_id = t.id AND v.mode = 'live'
           AND v.bundle_id = 14 AND v.reaction IS NOT NULL
           AND v.next_genuine_request_id IS NOT DISTINCT FROM f.next_genuine_request_id);
