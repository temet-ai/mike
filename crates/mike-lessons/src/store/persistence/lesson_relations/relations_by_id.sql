SELECT r.id AS "id!", r.uuid AS "uuid!",
       r.from_lesson_id AS "from_lesson_id!", lf.text AS "from_lesson_text!", lf.created_at AS "from_lesson_created_at!",
       r.to_lesson_id AS "to_lesson_id!", lt.text AS "to_lesson_text!", lt.created_at AS "to_lesson_created_at!",
       rk.code AS "kind!", rm.code AS "method!", rs.code AS "status!",
       r.candidate_distance, r.evidence_json AS "evidence_json!",
       r.proposer_model, r.proposer_rationale,
       r.adjudicator_model, r.adjudicator_agrees, r.adjudicator_rationale,
       r.detector_version AS "detector_version!", r.proposed_at AS "proposed_at!", r.decided_at, r.decided_by,
       r.created_at AS "created_at!"
  FROM memory.lesson_relations r
  JOIN memory.lessons lf ON lf.id = r.from_lesson_id
  JOIN memory.lessons lt ON lt.id = r.to_lesson_id
  JOIN memory.lesson_relation_kinds rk ON rk.id = r.kind_id
  JOIN memory.lesson_relation_methods rm ON rm.id = r.method_id
  JOIN memory.lesson_relation_statuses rs ON rs.id = r.status_id
 WHERE r.id = $1
