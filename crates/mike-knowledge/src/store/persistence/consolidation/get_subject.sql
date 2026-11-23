SELECT k.id AS "id!", k.project_id AS "project_id?", k.title AS "title!", k.text AS "text!", k.members AS "members!",
       COALESCE(c.carriers, 0)::integer AS "carriers!", k.changed_at AS "changed_at?", k.judged_at AS "judged_at?",
       d.created_at AS "newest_decision_at?", d.verdict AS "newest_verdict?", d.reason AS "newest_reason?"
  FROM memory.knowledge_subjects k
  LEFT JOIN LATERAL (
       SELECT count(DISTINCT s.item_id) AS carriers
         FROM memory.knowledge_statements s WHERE s.subject_id = k.id AND s.state = 'current') c ON true
  LEFT JOIN LATERAL (
       SELECT x.created_at, x.verdict, x.reason
         FROM memory.knowledge_consolidation_decisions x
        WHERE x.subject_id = k.id
        ORDER BY x.created_at DESC, x.id DESC LIMIT 1) d ON true
 WHERE k.id = $1
