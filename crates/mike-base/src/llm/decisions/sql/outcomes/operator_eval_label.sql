WITH answered AS (
    SELECT r.id AS record_id,
           substring(a.key FROM 6)::bigint AS turn_id,
           a.value ->> 'choice' AS prelabel
      FROM decisions.records r
      CROSS JOIN LATERAL jsonb_each(r.composed -> 'answers') a(key, value)
     WHERE r.point = 'label_prelabel'
       AND r.applied = 'labelled'
       AND r.created_at >= now() - interval '30 days'
       AND a.key ~ '^turn_[0-9]+$'
       AND a.value ->> 'choice' IS NOT NULL
),
human AS (
    SELECT DISTINCT ON (l.operator_turn_id) l.operator_turn_id, l.label
      FROM eval.labels l
     WHERE l.source = 'spa'
       AND l.subject_kind = 'turn'
       AND l.axis = 'reaction'
       AND l.action = 'assert'
       AND l.operator_turn_id IN (SELECT turn_id FROM answered)
     ORDER BY l.operator_turn_id, l.created_at DESC, l.id DESC
),
compared AS (
    SELECT a.record_id, a.turn_id, a.prelabel, h.label AS human,
           CASE WHEN a.prelabel = h.label THEN 'right' ELSE 'wrong' END AS verdict
      FROM answered a
      JOIN human h ON h.operator_turn_id = a.turn_id
),
one_per_verdict AS (
    SELECT DISTINCT ON (c.record_id, c.verdict) c.*
      FROM compared c
     ORDER BY c.record_id, c.verdict, c.turn_id
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT o.record_id, o.verdict, 'operator_gold_label',
       jsonb_build_object('turn_id', o.turn_id, 'prelabel', o.prelabel, 'human', o.human)
  FROM one_per_verdict o
 WHERE NOT EXISTS (SELECT 1 FROM decisions.outcomes x
                    WHERE x.record_id = o.record_id
                      AND x.label_source = 'operator_gold_label'
                      AND x.label = o.verdict)
ON CONFLICT (record_id, label_source, label) DO NOTHING
