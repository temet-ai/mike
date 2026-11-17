SELECT ot.closed_at AS "closed_at!",
       left(COALESCE(ot.user_prompt, ''), $3::int) AS "user_prompt!",
       left(COALESCE(ot.final_assistant_text, ''), $3::int) AS "final_text!",
       COALESCE(ot.files_touched, ARRAY[]::text[]) AS "files_touched!"
  FROM memory.operator_turns ot
 WHERE ot.session_id = $1
   AND ot.closed_at IS NOT NULL
 ORDER BY ot.closed_at DESC
 LIMIT $2
