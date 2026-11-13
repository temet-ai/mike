WITH due AS (
    SELECT r.id AS relation_id, r.status_id, r.adjudicator_rationale,
           CASE WHEN (a.created_at, a.id) <= (b.created_at, b.id) THEN a.id ELSE b.id END AS old_id,
           CASE WHEN (a.created_at, a.id) <= (b.created_at, b.id) THEN b.id ELSE a.id END AS new_id
      FROM memory.lesson_relations r
      JOIN memory.lesson_relation_kinds k ON k.id = r.kind_id AND k.code = 'contradicts'
      JOIN memory.lesson_relation_methods m ON m.id = r.method_id AND m.code = 'adjudicated'
      JOIN memory.lesson_relation_statuses s ON s.id = r.status_id AND s.code IN ('proposed', 'confirmed')
      JOIN memory.lessons a ON a.id = r.from_lesson_id
      JOIN memory.lessons b ON b.id = r.to_lesson_id
     WHERE r.adjudicator_agrees
), confirmed AS (
    UPDATE memory.lesson_relations r
       SET status_id = cs.id, decided_at = now(), decided_by = 'adjudicator'
      FROM due d, memory.lesson_relation_statuses cs, memory.lesson_relation_statuses ps
     WHERE r.id = d.relation_id AND cs.code = 'confirmed' AND ps.code = 'proposed' AND d.status_id = ps.id
    RETURNING r.id
), chosen AS (
    SELECT DISTINCT ON (d.old_id) d.old_id, d.new_id, d.adjudicator_rationale
      FROM due d
      JOIN memory.lessons n ON n.id = d.new_id AND n.retired_at IS NULL AND n.archived_at IS NULL
     ORDER BY d.old_id, d.relation_id DESC
)
UPDATE memory.lessons l
   SET retired_at = now(), retired_kind = 'superseded', retired_by_lesson_id = c.new_id,
       retired_reason = concat_ws(' ',
           format('Contradicted by the newer lesson %s, and the adjudicator agreed.', c.new_id),
           NULLIF(btrim(c.adjudicator_rationale), ''))
  FROM chosen c
 WHERE l.id = c.old_id AND l.retired_at IS NULL
RETURNING l.id
