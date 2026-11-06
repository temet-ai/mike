SELECT r.id AS request_id, r.created_at AS at, r.human_prompt AS text,
       k.code AS "kind_code?", c.rule AS "rule?", c.repeat_key AS "repeat_key?",
       c.classifier_version AS "classifier_version?", t.id AS "turn_id?", t.opened_by AS "opened_by?"
  FROM requests.requests r
  JOIN requests.turn_types tt ON tt.id = r.turn_type_id AND tt.code = 'user_prompt'
  JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                             AND o.code IN ('primary', 'primary_first')
  LEFT JOIN prompt_kinds.classifications c ON c.request_id = r.id
  LEFT JOIN prompt_kinds.kinds k ON k.id = c.kind_id
  LEFT JOIN memory.operator_turns t ON t.start_request_id = r.id
 WHERE r.session_id = $1
   AND r.created_at <= $2
   AND r.human_prompt <> ''
 ORDER BY r.id
