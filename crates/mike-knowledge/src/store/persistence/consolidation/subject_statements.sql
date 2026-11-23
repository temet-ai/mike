SELECT s.id AS "statement_id!", s.item_id AS "item_id!", s.ordinal AS "ordinal!", s.text AS "text!",
       s.passage_start AS "passage_start?", s.passage_end AS "passage_end?",
       e.reference AS "carrier_reference!", i.kind AS "kind!", a.source_name AS "source_name?",
       s.stated_at AS "stated_at!"
  FROM memory.knowledge_statements s
  JOIN memory.knowledge_items i ON i.id = s.item_id
  JOIN memory.knowledge_embedding_state e ON e.item_id = s.item_id AND e.reference IS NOT NULL
  LEFT JOIN memory.knowledge_artifact_identity_v a ON i.kind = 'artifact' AND a.revision_id = i.source_id
 WHERE s.subject_id = $1 AND s.state = 'current'
 ORDER BY s.stated_at, s.id
