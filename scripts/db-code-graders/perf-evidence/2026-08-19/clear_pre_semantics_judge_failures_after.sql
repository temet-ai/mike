UPDATE memory.operator_turns
   SET judge_failure_reason = NULL,
       judge_attempt_count = 0,
       judge_next_attempt_at = NULL
 WHERE judge_failure_reason = 'permanent'
