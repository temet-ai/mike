WITH k AS (
    SELECT turn_id, as_of FROM unnest($1::bigint[], $2::timestamptz[]) AS k(turn_id, as_of)
)
SELECT k.turn_id AS "turn_id!", u.id AS "use_id!", u.created_at AS "at!", u.tool_name AS "tool_name!",
       coalesce(u.input, '{}'::jsonb) AS "input!",
       (r.id IS NOT NULL) AS "has_result!"
  FROM k
  JOIN requests.requests q ON q.operator_turn_id = k.turn_id
  JOIN tools.uses u ON u.request_id = q.id AND u.created_at <= k.as_of
  LEFT JOIN tools.results r ON r.tool_use_id = u.id AND r.created_at <= k.as_of
 ORDER BY k.turn_id, u.created_at, u.request_id, u.ord
