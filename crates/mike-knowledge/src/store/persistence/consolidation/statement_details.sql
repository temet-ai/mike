SELECT s.id AS "statement_id!", s.item_id AS "item_id!", s.ordinal AS "ordinal!", s.text AS "text!",
       s.passage_start AS "passage_start?", s.passage_end AS "passage_end?",
       COALESCE(e.reference, '') AS "carrier_reference!", i.kind AS "kind!", a.source_name AS "source_name?",
       i.source_id AS "source_id!", i.project_id AS "project_id?", s.stated_at AS "stated_at!",
       s.subject_id AS "subject_id?", s.state AS "state!", a.relative_path AS "relative_path?", a.scope AS "scope?",
       left(r.body, 600) AS "head?",
       s.resolved_by_statement_id AS "resolved_by_statement_id?",
       CASE WHEN s.passage_start IS NOT NULL AND r.body IS NOT NULL THEN
            convert_from(substring(convert_to(r.body, 'UTF8') FROM s.passage_start + 1
                                   FOR s.passage_end - s.passage_start), 'UTF8') END AS "passage?",
       CASE WHEN i.kind = 'lesson' THEN l.archived_at IS NULL AND l.retired_at IS NULL END AS "lesson_active?"
  FROM memory.knowledge_statements s
  JOIN memory.knowledge_items i ON i.id = s.item_id
  LEFT JOIN memory.knowledge_embedding_state e ON e.item_id = s.item_id
  LEFT JOIN memory.knowledge_artifact_identity_v a ON i.kind = 'artifact' AND a.revision_id = i.source_id
  LEFT JOIN memory.knowledge_revisions r ON i.kind = 'artifact' AND r.id = i.source_id
  LEFT JOIN memory.lessons l ON i.kind = 'lesson' AND l.id = i.source_id
 WHERE s.id = ANY($1::bigint[])
 ORDER BY s.stated_at, s.id
