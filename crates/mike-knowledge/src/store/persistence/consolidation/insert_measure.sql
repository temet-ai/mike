WITH cur_stmts AS (
    SELECT s.id, s.item_id, s.subject_id, s.text
      FROM memory.knowledge_statements s
      JOIN memory.knowledge_items i ON i.id = s.item_id
     WHERE s.state = 'current' AND i.project_id IS NOT DISTINCT FROM $2::bigint
), scoped AS (
    SELECT k.id FROM memory.knowledge_subjects k
     WHERE k.members > 0 AND k.project_id IS NOT DISTINCT FROM $2::bigint
), shared AS (
    SELECT c.subject_id FROM cur_stmts c WHERE c.subject_id IS NOT NULL
     GROUP BY c.subject_id HAVING count(DISTINCT c.item_id) > 1
), open_conflicts AS (
    SELECT count(DISTINCT (LEAST(d.statement_id, d.counterpart_statement_id),
                           GREATEST(d.statement_id, d.counterpart_statement_id))) AS n
      FROM memory.knowledge_consolidation_decisions d
      JOIN cur_stmts a ON a.id = d.statement_id
      JOIN cur_stmts b ON b.id = d.counterpart_statement_id
     WHERE d.verdict = 'conflict_open' AND d.application <> 'undone'
), judged AS (
    SELECT count(*) AS total, count(*) FILTER (WHERE d.verdict = 'misgrouped') AS misgrouped
      FROM memory.knowledge_consolidation_decisions d
     WHERE d.run_id = $1 AND d.project_id IS NOT DISTINCT FROM $2::bigint
)
INSERT INTO memory.knowledge_consolidation_measures
    (run_id, project_id, statements_current, subjects, subjects_shared, conflicts_open, guidance_chars,
     misgrouped_share, split_subject_share, grouping_distance)
SELECT $1, $2::bigint, (SELECT count(*) FROM cur_stmts), (SELECT count(*) FROM scoped),
       (SELECT count(*) FROM shared), (SELECT n FROM open_conflicts),
       (SELECT COALESCE(sum(char_length(c.text)), 0) FROM cur_stmts c),
       CASE WHEN j.total = 0 THEN 0 ELSE j.misgrouped::real / j.total END,
       $3::real, $4::real
  FROM judged j
RETURNING id, measured_at, statements_current, subjects, subjects_shared, conflicts_open, guidance_chars,
          misgrouped_share, split_subject_share, grouping_distance
