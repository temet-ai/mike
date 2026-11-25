SELECT d.lesson_id, d.text, d.route, d.request_id, r.operator_turn_id AS turn_id, d.supplied_at
  FROM (SELECT DISTINCT ON (si.lesson_id)
               si.lesson_id, si.item_text AS text, si.route, si.request_id, si.supplied_at, si.id
          FROM memory.supplied_items si
         WHERE si.session_id = $1
           AND si.item_kind = 'lesson'
           AND si.supplied_at <= $2
         ORDER BY si.lesson_id, si.supplied_at DESC, si.id DESC) d
  JOIN requests.requests r ON r.id = d.request_id
 ORDER BY d.supplied_at DESC, d.id DESC
 LIMIT 20
