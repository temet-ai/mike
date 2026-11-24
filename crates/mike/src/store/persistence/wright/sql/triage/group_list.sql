SELECT g.id, g.name, g.created_at, count(q.cause_id) AS causes
  FROM wright.cause_groups g
  LEFT JOIN wright.cause_queue q ON q.group_id = g.id
 GROUP BY g.id, g.name, g.created_at
 ORDER BY lower(g.name), g.id
