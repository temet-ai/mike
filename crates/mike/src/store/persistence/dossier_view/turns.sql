SELECT t.id AS turn_id, t.start_request_id, t.created_at,
       CASE WHEN t.closed_at <= $2 THEN t.closed_at END AS "closed_at?",
       t.opened_by,
       COALESCE(t.user_prompt, '') AS "prompt!",
       CASE WHEN t.closed_at <= $2 THEN COALESCE(t.final_assistant_text, '') ELSE '' END AS "final_text!",
       COALESCE(array_length(t.files_touched, 1), 0)::int AS "files_touched_count!",
       f.text_chars AS "text_chars?", f.text_cut_at AS "text_cut_at?",
       f.text_spans AS "text_spans?", f.last_text_origin AS "last_text_origin?",
       f.stop_request_id AS "stop_request_id?", f.stop_reason AS "stop_reason?",
       f.aborted AS "aborted?", f.interrupted AS "interrupted?", f.close_cause AS "close_cause?",
       f.checks_run AS "checks_run?", f.checks_count AS "checks_count?",
       f.checks_failed AS "checks_failed?", f.ended_on AS "ended_on?",
       f.ended_on_version AS "ended_on_version?", f.builder_version AS "builder_version?",
       f.built_at AS "built_at?", f.max_request_id AS "max_request_id?",
       f.session_class AS "session_class?", f.session_class_rule AS "session_class_rule?"
  FROM memory.operator_turns t
  LEFT JOIN memory.turn_facts f
         ON f.operator_turn_id = t.id
        AND t.closed_at <= $2
 WHERE t.session_id = $1
   AND t.created_at <= $2
 ORDER BY t.start_request_id, t.id
