UPDATE session_judges.attempts
   SET last_attempt_at = GREATEST(last_attempt_at, now())
 WHERE session_id = 1237996 AND judge_model = 'google/gemini-3.5-flash-lite' AND rubric_version = 2
   AND attempt_count = -1
