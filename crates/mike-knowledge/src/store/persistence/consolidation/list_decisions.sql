SELECT d.id AS "id!", d.run_id AS "run_id!", d.project_id AS "project_id?", d.subject_id AS "subject_id?",
       k.title AS "subject_title?", d.statement_id AS "statement_id?", d.item_id AS "item_id?",
       d.carrier_reference AS "carrier_reference!", d.verdict AS "verdict!",
       d.counterpart_statement_id AS "counterpart_statement_id?", d.counterpart_reference AS "counterpart_reference?",
       d.text_before AS "text_before!", d.text_after AS "text_after?", d.reason AS "reason!",
       d.evidence AS "evidence!", d.model AS "model!", d.application AS "application!",
       d.applied_at AS "applied_at?", d.error AS "error?", d.created_at AS "created_at!"
  FROM memory.knowledge_consolidation_decisions d
  LEFT JOIN memory.knowledge_subjects k ON k.id = d.subject_id
 WHERE (d.project_id = $1 OR d.project_id IS NULL)
   AND ($2::bigint IS NULL OR d.subject_id = $2)
   AND ($3::timestamptz IS NULL OR (d.created_at, d.id) < ($3, $4))
   AND ($6::text[] IS NULL OR d.verdict = ANY($6))
   AND (NOT $7::boolean OR (
        d.verdict = 'conflict_open' AND d.application <> 'undone'
        AND EXISTS (SELECT 1 FROM memory.knowledge_statements a
                     WHERE a.id = d.statement_id AND a.state = 'current')
        AND EXISTS (SELECT 1 FROM memory.knowledge_statements b
                     WHERE b.id = d.counterpart_statement_id AND b.state = 'current')
        AND NOT EXISTS (
            SELECT 1 FROM memory.knowledge_consolidation_decisions x
             WHERE x.verdict = 'conflict_open' AND x.id > d.id
               AND LEAST(x.statement_id, x.counterpart_statement_id)
                   = LEAST(d.statement_id, d.counterpart_statement_id)
               AND GREATEST(x.statement_id, x.counterpart_statement_id)
                   = GREATEST(d.statement_id, d.counterpart_statement_id))))
 ORDER BY d.created_at DESC, d.id DESC
 LIMIT $5
