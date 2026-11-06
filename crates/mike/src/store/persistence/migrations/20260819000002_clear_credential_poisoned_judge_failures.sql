-- #151: extends 20260819000001's reasoning to rows poisoned since — a 401/402/403 credential failure was still misclassified permanent until this deploy, so these too predate the exclusion semantics and are cleared, not corrected as bad data.

UPDATE memory.operator_turns
   SET judge_failure_reason = NULL,
       judge_attempt_count = 0,
       judge_next_attempt_at = NULL
 WHERE judge_failure_reason = 'permanent';
