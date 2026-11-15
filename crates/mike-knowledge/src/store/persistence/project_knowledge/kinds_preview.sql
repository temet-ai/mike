SELECT COALESCE(b.reference, '') AS "reference!",
       btrim(left(concat_ws(': ', regexp_replace(b.label, '\s+', ' ', 'g'),
                            NULLIF(btrim(regexp_replace(b.name, '\s+', ' ', 'g')), '')), 120)) AS "header!",
       COALESCE(b.body, '') AS "body!"
  FROM (SELECT i.source_id FROM memory.knowledge_items i WHERE i.kind = $1 ORDER BY i.source_id DESC LIMIT 1) one
 CROSS JOIN LATERAL memory.knowledge_item_bodies($1, ARRAY[one.source_id]) b
 LIMIT 1
