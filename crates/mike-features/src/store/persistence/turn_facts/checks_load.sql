WITH scope AS (
    SELECT t.id AS turn_id, t.session_id, t.start_request_id,
           lead(t.start_request_id) OVER (
               PARTITION BY t.session_id ORDER BY t.start_request_id
           ) AS next_start
      FROM memory.operator_turns t
     WHERE t.session_id IN (SELECT s.session_id FROM memory.operator_turns s WHERE s.id = ANY($1))
), wanted AS (
    SELECT turn_id, session_id, start_request_id, next_start FROM scope WHERE turn_id = ANY($1)
)
SELECT w.turn_id AS "turn_id!",
       u.wire_tool_use_id AS tool_use_id,
       u.request_id,
       r.created_at AS at,
       (u.input ->> 'command') AS "command!",
       res.content AS "result_content?",
       res.is_error AS "result_is_error?",
       wi.id AS "work_item_id?",
       wi.state AS "work_item_state?",
       wi.exit_code AS "work_item_exit_code?",
       wi.finished_at AS "work_item_finished_at?"
  FROM wanted w
  JOIN requests.requests r
    ON r.session_id = w.session_id
   AND r.id >= w.start_request_id
   AND (w.next_start IS NULL OR r.id < w.next_start)
  JOIN tools.uses u ON u.request_id = r.id AND u.tool_name = 'Bash'
  LEFT JOIN tools.results res ON res.tool_use_id = u.id
  LEFT JOIN agents.work_items wi ON wi.wire_tool_use_id = u.wire_tool_use_id
 WHERE u.input ->> 'command' IS NOT NULL
 ORDER BY w.turn_id, u.request_id, u.ord
