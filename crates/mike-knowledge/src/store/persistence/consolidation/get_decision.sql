SELECT d.id AS "id!", d.run_id AS "run_id!", d.project_id AS "project_id?", d.subject_id AS "subject_id?",
       k.title AS "subject_title?", d.statement_id AS "statement_id?", d.item_id AS "item_id?",
       d.carrier_reference AS "carrier_reference!", d.verdict AS "verdict!",
       d.counterpart_statement_id AS "counterpart_statement_id?", d.counterpart_reference AS "counterpart_reference?",
       d.text_before AS "text_before!", d.text_after AS "text_after?", d.reason AS "reason!",
       d.evidence AS "evidence!", d.model AS "model!", d.application AS "application!",
       d.applied_at AS "applied_at?", d.error AS "error?", d.created_at AS "created_at!"
  FROM memory.knowledge_consolidation_decisions d
  LEFT JOIN memory.knowledge_subjects k ON k.id = d.subject_id
 WHERE d.id = $1
