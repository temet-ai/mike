WITH subjects AS (
    SELECT r.id AS record_id, r.created_at, l.id::bigint AS lesson_id
      FROM decisions.records r
      CROSS JOIN LATERAL jsonb_array_elements_text(
               COALESCE(r.composed -> 'verdict' -> 'lesson_ids', '[]'::jsonb)
           ) l(id)
     WHERE r.point = ANY($1)
       AND r.created_at >= now() - interval '30 days'
       AND r.applied = 'chosen'
),
states AS (
    SELECT s.record_id, s.created_at,
           bool_or(ar.code IN ('operator', 'rejected_by_operator', 'not_a_lesson')) AS rejected,
           count(*) AS lessons
      FROM subjects s
      JOIN memory.lessons ls ON ls.id = s.lesson_id
      LEFT JOIN memory.lesson_archived_reasons ar ON ar.id = ls.archived_reason_id
     GROUP BY s.record_id, s.created_at
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT st.record_id,
       CASE WHEN COALESCE(st.rejected, false) THEN 'wrong' ELSE 'right' END,
       'lesson_operator_verdict',
       jsonb_build_object('lessons', st.lessons, 'rejected_by_operator', COALESCE(st.rejected, false))
  FROM states st
 WHERE (COALESCE(st.rejected, false) OR st.created_at <= now() - interval '7 days')
   AND NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = st.record_id AND o.label_source = 'lesson_operator_verdict')
ON CONFLICT (record_id, label_source, label) DO NOTHING
