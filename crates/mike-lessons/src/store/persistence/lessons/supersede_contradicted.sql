WITH agreed AS (
    SELECT r.id AS relation_id, r.status_id, r.adjudicator_rationale, k.code AS kind,
           r.from_lesson_id, r.to_lesson_id, a.created_at AS a_created, b.created_at AS b_created
      FROM memory.lesson_relations r
      JOIN memory.lesson_relation_kinds k
        ON k.id = r.kind_id AND k.code IN ('contradicts', 'supersedes', 'refines', 'recurs')
      JOIN memory.lesson_relation_methods m ON m.id = r.method_id
      JOIN memory.lesson_relation_statuses s ON s.id = r.status_id AND s.code IN ('proposed', 'confirmed')
      JOIN memory.lessons a ON a.id = r.from_lesson_id
      JOIN memory.lessons b ON b.id = r.to_lesson_id
     WHERE (m.code = 'adjudicated' AND r.adjudicator_agrees AND r.detector_version = $1)
        OR (m.code = 'declared' AND s.code = 'confirmed' AND k.code = 'supersedes')
), due AS (
    SELECT relation_id, adjudicator_rationale, kind,
           CASE WHEN kind = 'supersedes' THEN to_lesson_id
                WHEN (a_created, from_lesson_id) <= (b_created, to_lesson_id) THEN from_lesson_id
                ELSE to_lesson_id END AS old_id,
           CASE WHEN kind = 'supersedes' THEN from_lesson_id
                WHEN (a_created, from_lesson_id) <= (b_created, to_lesson_id) THEN to_lesson_id
                ELSE from_lesson_id END AS new_id
      FROM agreed
     WHERE kind IN ('contradicts', 'supersedes')
), confirmed AS (
    UPDATE memory.lesson_relations r
       SET status_id = cs.id, decided_at = now(), decided_by = 'adjudicator'
      FROM agreed g, memory.lesson_relation_statuses cs, memory.lesson_relation_statuses ps
     WHERE r.id = g.relation_id AND cs.code = 'confirmed' AND ps.code = 'proposed' AND g.status_id = ps.id
    RETURNING r.id
), chosen AS (
    SELECT DISTINCT ON (d.old_id) d.old_id, d.new_id, d.kind, d.adjudicator_rationale
      FROM due d
      JOIN memory.lessons n ON n.id = d.new_id AND n.retired_at IS NULL AND n.archived_at IS NULL
     ORDER BY d.old_id, d.relation_id DESC
)
UPDATE memory.lessons l
   SET retired_at = now(), retired_kind = 'superseded', retired_by_lesson_id = c.new_id,
       retired_reason = concat_ws(' ',
           format(CASE WHEN c.kind = 'supersedes'
                       THEN 'Superseded by lesson %s, and the adjudicator agreed.'
                       ELSE 'Contradicted by the newer lesson %s, and the adjudicator agreed.' END,
                  c.new_id),
           NULLIF(btrim(c.adjudicator_rationale), ''))
  FROM chosen c
 WHERE l.id = c.old_id AND l.retired_at IS NULL
RETURNING l.id
