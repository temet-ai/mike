SELECT r.id AS "id!", r.created_at AS "created_at!",
       oo.code AS "old_origin!",
       CASE WHEN w.is_workflow THEN 'workflow_subagent' ELSE 'subagent' END AS "new_origin!",
       oa.code AS "old_author?",
       'agent' AS "new_author!"
  FROM requests.requests r
  JOIN requests.turn_origins oo ON oo.id = r.turn_origin_id
  LEFT JOIN requests.author_kinds oa ON oa.id = r.author_kind_id
  LEFT JOIN requests.request_bodies_v b ON b.request_id = r.id
 CROSS JOIN LATERAL (
       SELECT COALESCE(b.system_text LIKE ANY($2::text[]), false)
           OR COALESCE(CASE jsonb_typeof(b.body -> 'messages' -> 0 -> 'content')
                  WHEN 'string' THEN (b.body -> 'messages' -> 0 ->> 'content') ~ $3
                  WHEN 'array' THEN EXISTS (
                       SELECT 1
                         FROM jsonb_array_elements(b.body -> 'messages' -> 0 -> 'content') blk
                        WHERE blk ->> 'type' = 'text' AND (blk ->> 'text') ~ $3)
              END, false) AS is_workflow
      ) w
 WHERE r.id = ANY($1)
 ORDER BY r.id
