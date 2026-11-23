SELECT m.id AS "id!", m.run_id AS "run_id!", m.project_id AS "project_id?", m.measured_at AS "measured_at!",
       m.statements_current AS "statements_current!", m.subjects AS "subjects!",
       m.subjects_shared AS "subjects_shared!", m.conflicts_open AS "conflicts_open!",
       m.guidance_chars AS "guidance_chars!", m.misgrouped_share AS "misgrouped_share!",
       m.split_subject_share AS "split_subject_share!", m.grouping_distance AS "grouping_distance!"
  FROM memory.knowledge_consolidation_measures m
 WHERE m.project_id IS NOT DISTINCT FROM $1 AND ($2::timestamptz IS NULL OR m.measured_at >= $2)
 ORDER BY m.measured_at, m.id
 LIMIT $3
