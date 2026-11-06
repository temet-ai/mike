UPDATE session_judges.attempts
   SET consecutive_failures = consecutive_failures
 WHERE session_id = 1237996 AND judge_model = 'google/gemini-3.5-flash-lite' AND rubric_version = 2
