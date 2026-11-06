SELECT id                                        AS turn_id,
        COALESCE(user_prompt, '')           AS user_prompt,
        COALESCE(final_assistant_text, '')  AS final_assistant_text,
        COALESCE(tool_call_count, 0)::int   AS tool_call_count,
        COALESCE(tool_error_count, 0)::int  AS tool_error_count
   FROM memory.operator_turns
  WHERE closed_at   IS NOT NULL
    AND outcome_id  = 1
    AND (judge_rubric_version IS NULL OR judge_rubric_version <> 'r1-2026-06-22')
    AND judge_failure_reason IS DISTINCT FROM 'permanent'
    AND (judge_next_attempt_at IS NULL OR judge_next_attempt_at <= NOW())
  ORDER BY closed_at DESC
  LIMIT 50
