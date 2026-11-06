WITH l AS (
    SELECT l.*,
           CASE l.tier WHEN 'gold' THEN 1 WHEN 'relayed' THEN 2 WHEN 'seed' THEN 3 ELSE 4 END AS tier_rank
      FROM eval.labels_as_of(now()) l
     WHERE l.subject_kind = 'session_end'
       AND l.axis = 'session_end'
       AND l.tier = ANY(ARRAY['seed','gold']::text[])
       AND (l.tier = 'gold' OR l.source = 'fleet_mirror/session_ends')
),
best AS (
    SELECT DISTINCT ON (subject_id) *
      FROM l
     ORDER BY subject_id, tier_rank, created_at DESC, id DESC
)
SELECT b.subject_id AS end_turn_id, t.session_id, b.id AS label_id, b.label, b.tier,
       t.closed_at, s.session_uuid::text AS session_uuid
  FROM best b
  JOIN memory.operator_turns t ON t.id = b.subject_id
  JOIN sessions.sessions s ON s.id = t.session_id
 ORDER BY b.subject_id
