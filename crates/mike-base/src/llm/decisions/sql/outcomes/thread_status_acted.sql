WITH suggested AS (
    SELECT r.id AS record_id, r.created_at, th.status, th.updated_at
      FROM decisions.records r
      JOIN work.threads th
        ON th.project_id = r.project_id
       AND th.slug = r.composed -> 'verdict' ->> 'slug'
     WHERE r.point = 'thread_status'
       AND r.created_at >= now() - interval '30 days'
       AND r.applied = 'suggested'
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT s.record_id,
       CASE WHEN s.status IN ('done', 'parked') AND s.updated_at > s.created_at
            THEN 'right' ELSE 'wrong' END,
       'operator_status_change',
       jsonb_build_object('status_now', s.status)
  FROM suggested s
 WHERE ((s.status IN ('done', 'parked') AND s.updated_at > s.created_at)
        OR s.created_at <= now() - interval '14 days')
   AND NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = s.record_id AND o.label_source = 'operator_status_change')
ON CONFLICT (record_id, label_source, label) DO NOTHING
