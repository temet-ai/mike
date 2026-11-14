WITH suggested AS (
    SELECT r.id AS record_id, r.created_at,
           (r.composed -> 'verdict' ->> 'source_project_id')::bigint AS source_id,
           (r.composed -> 'verdict' ->> 'target_project_id')::bigint AS target_id
      FROM decisions.records r
     WHERE r.point = 'project_merge'
       AND r.created_at >= now() - interval '30 days'
       AND r.applied = 'suggested'
       AND r.composed -> 'verdict' ->> 'source_project_id' IS NOT NULL
       AND r.composed -> 'verdict' ->> 'target_project_id' IS NOT NULL
),
acted AS (
    SELECT s.record_id, s.created_at,
           COALESCE(src.merged_into_id = s.target_id AND src.merged_at >= s.created_at, false)
        OR COALESCE(tgt.merged_into_id = s.source_id AND tgt.merged_at >= s.created_at, false)
               AS merged
      FROM suggested s
      LEFT JOIN projects.projects src ON src.id = s.source_id
      LEFT JOIN projects.projects tgt ON tgt.id = s.target_id
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT a.record_id,
       CASE WHEN a.merged THEN 'right' ELSE 'wrong' END,
       'operator_merge',
       jsonb_build_object('merged', a.merged)
  FROM acted a
 WHERE (a.merged OR a.created_at <= now() - interval '14 days')
   AND NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = a.record_id AND o.label_source = 'operator_merge')
ON CONFLICT (record_id, label_source, label) DO NOTHING
