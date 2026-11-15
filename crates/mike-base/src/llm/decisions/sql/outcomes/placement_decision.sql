WITH guessed AS (
    SELECT r.id AS record_id, r.created_at,
           substring(a.key FROM 10)::bigint AS lesson_id,
           COALESCE((a.value -> 'probabilities' ->> 'accept')::float8,
                    CASE WHEN a.value ->> 'choice' = 'accept'
                         THEN (a.value ->> 'confidence')::float8
                         ELSE 0 END) AS accept
      FROM decisions.records r
      CROSS JOIN LATERAL jsonb_each(r.composed -> 'answers') a(key, value)
     WHERE r.point = 'proposal_order'
       AND r.applied = 'ordered'
       AND r.created_at >= now() - interval '30 days'
       AND a.key ~ '^proposal_[0-9]+$'
),
compared AS (
    SELECT g.record_id, g.lesson_id, g.accept, pr.decision,
           CASE WHEN (g.accept >= 0.5) = (pr.decision = 'accepted')
                THEN 'right' ELSE 'wrong' END AS verdict
      FROM guessed g
      JOIN memory.lesson_placement_reviews pr
        ON pr.lesson_id = g.lesson_id
       AND pr.decided_at >= g.created_at
     WHERE g.accept IS NOT NULL
       AND pr.decision IN ('accepted', 'declined')
),
one_per_verdict AS (
    SELECT DISTINCT ON (c.record_id, c.verdict) c.*
      FROM compared c
     ORDER BY c.record_id, c.verdict, c.lesson_id
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT o.record_id, o.verdict, 'operator_proposal_decision',
       jsonb_build_object('lesson_id', o.lesson_id, 'accept_probability', o.accept,
                          'decision', o.decision)
  FROM one_per_verdict o
 WHERE NOT EXISTS (SELECT 1 FROM decisions.outcomes x
                    WHERE x.record_id = o.record_id
                      AND x.label_source = 'operator_proposal_decision'
                      AND x.label = o.verdict)
ON CONFLICT (record_id, label_source, label) DO NOTHING
