WITH suggested AS (
    SELECT r.id AS record_id, r.created_at, th.id AS thread_id, th.status
      FROM decisions.records r
      JOIN work.threads th
        ON th.project_id = r.project_id
       AND th.slug = r.composed -> 'verdict' ->> 'slug'
     WHERE r.point = 'thread_status'
       AND r.created_at >= now() - interval '30 days'
       AND r.applied = 'suggested'
       AND th.status NOT IN ('done', 'parked')
),
moving AS (
    SELECT DISTINCT ON (s.record_id)
           s.record_id, l.id AS log_id, l.created_at AS entry_at
      FROM suggested s
      JOIN work.thread_log l
        ON l.thread_id = s.thread_id
       AND l.kind = 'checkpoint'
       AND l.created_at > s.created_at
       AND l.created_at <= s.created_at + interval '24 hours'
     ORDER BY s.record_id, l.created_at, l.id
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT m.record_id, 'wrong', 'thread_kept_moving',
       jsonb_build_object('thread_log_id', m.log_id, 'checkpoint_at', m.entry_at)
  FROM moving m
 WHERE NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = m.record_id AND o.label_source = 'thread_kept_moving')
ON CONFLICT (record_id, label_source, label) DO NOTHING
