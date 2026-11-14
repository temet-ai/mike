WITH mapped AS (
    SELECT r.id AS record_id, r.request_id, r.project_id, t.topic
      FROM decisions.records r
      CROSS JOIN LATERAL jsonb_array_elements_text(
               COALESCE(r.composed -> 'verdict' -> 'mapped_topics', '[]'::jsonb)
           ) t(topic)
     WHERE r.point = 'recall_topic'
       AND r.created_at >= now() - interval '30 days'
       AND r.applied = 'chosen'
       AND r.request_id IS NOT NULL
       AND r.project_id IS NOT NULL
),
verdicts AS (
    SELECT m.record_id,
           bool_or(COALESCE(v.hand_verdict, v.verdict) = 'used') AS useful,
           count(*) AS items
      FROM mapped m
      JOIN memory.lessons ls ON ls.project_id = m.project_id AND ls.topic = m.topic
      JOIN memory.supplied_items si
        ON si.request_id = m.request_id
       AND si.item_kind = 'lesson'
       AND si.lesson_id = ls.id
      JOIN memory.supplied_item_verdicts v ON v.supplied_item_id = si.id
     GROUP BY m.record_id
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT v.record_id,
       CASE WHEN v.useful THEN 'right' ELSE 'wrong' END,
       'recalled_lesson_verdict',
       jsonb_build_object('judged_items', v.items, 'any_used', v.useful)
  FROM verdicts v
 WHERE NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = v.record_id AND o.label_source = 'recalled_lesson_verdict')
ON CONFLICT (record_id, label_source, label) DO NOTHING
