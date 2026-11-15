WITH picked AS MATERIALIZED (
    SELECT i.id, i.kind, i.source_id, i.changed_at
      FROM memory.knowledge_items i
      JOIN memory.knowledge_kinds k ON k.kind = i.kind AND k.enabled
     WHERE i.needs_embedding AND i.ready_at <= now()
     ORDER BY i.ready_at, i.id
     LIMIT $1
), bodies AS (
    SELECT g.kind, b.source_id, b.project_id, b.reference, b.label, b.name, b.body
      FROM (SELECT kind, array_agg(source_id) AS ids FROM picked GROUP BY kind) g
     CROSS JOIN LATERAL memory.knowledge_item_bodies(g.kind, g.ids) b
)
SELECT DISTINCT ON (p.id)
       p.id AS "item_id!", p.changed_at AS "changed_at!", p.kind AS "source_kind!", p.source_id AS "source_id!",
       b.project_id, b.reference,
       btrim(left(concat_ws(': ', regexp_replace(b.label, '\s+', ' ', 'g'),
                            NULLIF(btrim(regexp_replace(b.name, '\s+', ' ', 'g')), '')), 120)) AS header,
       b.body
  FROM picked p
  LEFT JOIN bodies b ON b.kind = p.kind AND b.source_id = p.source_id
 ORDER BY p.id
