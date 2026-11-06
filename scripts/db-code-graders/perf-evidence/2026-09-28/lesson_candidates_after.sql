SELECT l.id, l.project_id, p.project_path, lk.code AS kind, l.text,
       lsrc.code AS source, ls.code AS stage, l.source_turn_id, l.created_at,
       COALESCE(ec.evidence_count, 0) AS evidence_count
  FROM memory.lessons l
  LEFT JOIN projects.projects p ON p.id = l.project_id
  JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
  JOIN memory.lesson_stages ls ON ls.id = l.stage_id
  JOIN memory.lesson_sources lsrc ON lsrc.id = l.source_id
  LEFT JOIN (
      SELECT lesson_id, count(*) AS evidence_count
        FROM memory.lesson_proposal_evidence
       WHERE lesson_id IS NOT NULL
       GROUP BY lesson_id
  ) ec ON ec.lesson_id = l.id
 WHERE ls.code = 'raw' AND lsrc.code IN ('judge', 'directive')
   AND (NULL::bigint IS NULL OR l.id = NULL::bigint)
 ORDER BY l.created_at DESC
 LIMIT 100;
