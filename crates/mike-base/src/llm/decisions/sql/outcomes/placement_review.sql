WITH placed AS (
    SELECT r.id AS record_id, r.created_at, l.id::bigint AS lesson_id
      FROM decisions.records r
      CROSS JOIN LATERAL jsonb_array_elements_text(
               COALESCE(r.composed -> 'verdict' -> 'lesson_ids', '[]'::jsonb)
           ) l(id)
     WHERE r.point = 'lesson_relate'
       AND r.created_at >= now() - interval '30 days'
       AND r.applied = 'chosen'
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT p.record_id,
       CASE WHEN pr.decision = 'accepted' THEN 'right' ELSE 'wrong' END,
       'placement_review',
       jsonb_build_object('lesson_id', p.lesson_id, 'decision', pr.decision)
  FROM placed p
  JOIN memory.lesson_placement_reviews pr
    ON pr.lesson_id = p.lesson_id
   AND pr.decided_at >= p.created_at
 WHERE NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = p.record_id AND o.label_source = 'placement_review')
ON CONFLICT (record_id, label_source, label) DO NOTHING
