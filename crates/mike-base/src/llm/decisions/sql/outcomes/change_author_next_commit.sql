WITH picked AS (
    SELECT r.id AS record_id, r.created_at,
           r.state ->> 'changed_file' AS changed_file,
           r.answers -> 'who' ->> 'choice' AS pick,
           ARRAY(
               SELECT substring(c ->> 'id' FROM 2)::bigint
                 FROM jsonb_array_elements(r.state -> 'candidate_sessions') c
                WHERE c ->> 'id' ~ '^s[0-9]+$'
           ) AS candidates
      FROM decisions.records r
     WHERE r.point = 'change_author'
       AND r.created_at >= now() - interval '30 days'
       AND r.applied = 'chosen'
       AND r.answers -> 'who' ->> 'choice' ~ '^(s[0-9]+|unknown)$'
       AND r.state ->> 'changed_file' IS NOT NULL
),
committed AS (
    SELECT DISTINCT ON (p.record_id)
           p.record_id, p.pick, p.candidates, ce.id AS event_id, ce.session_id AS committer
      FROM picked p
      JOIN workspace.change_events ce
        ON ce.kind = 'commit'
       AND ce.session_id = ANY (p.candidates)
       AND ce.at > p.created_at
       AND ce.at <= p.created_at + interval '6 hours'
       AND (ce.detail ->> 'author') IN ('fleet_seat_trailer', 'shell')
       AND (ce.checkout = p.changed_file OR p.changed_file = ANY (ce.paths))
     ORDER BY p.record_id, ce.at, ce.id
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT c.record_id,
       CASE WHEN c.pick = 's' || c.committer THEN 'right' ELSE 'wrong' END,
       'next_commit_author',
       jsonb_build_object('change_event_id', c.event_id, 'committer_session', c.committer,
                          'picked', c.pick)
  FROM committed c
 WHERE NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = c.record_id AND o.label_source = 'next_commit_author')
ON CONFLICT (record_id, label_source, label) DO NOTHING
