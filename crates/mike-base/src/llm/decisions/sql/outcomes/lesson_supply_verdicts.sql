WITH calls AS (
    SELECT r.id AS record_id, r.request_id, r.created_at, rq.session_id,
           r.composed -> 'verdict' -> 'keep' AS keep,
           r.composed -> 'verdict' -> 'hold' AS hold
      FROM decisions.records r
      LEFT JOIN requests.requests rq ON rq.id = r.request_id
     WHERE r.point = 'lesson_supply'
       AND r.created_at >= now() - interval '30 days'
       AND r.request_id IS NOT NULL
       AND r.applied IN ('sent', 'held')
),
sent AS (
    SELECT c.record_id, 'sent_lesson_verdict'::text AS label_source,
           bool_or(COALESCE(v.hand_verdict, v.verdict) = 'used') AS useful,
           count(*) AS items
      FROM calls c
      CROSS JOIN LATERAL jsonb_array_elements_text(COALESCE(c.keep, '[]'::jsonb)) k(id)
      JOIN memory.supplied_items si
        ON si.request_id = c.request_id
       AND si.item_kind = 'lesson'
       AND si.lesson_id = k.id::bigint
      JOIN memory.supplied_item_verdicts v ON v.supplied_item_id = si.id
     GROUP BY c.record_id
),
held AS (
    SELECT c.record_id, 'held_lesson_verdict'::text AS label_source,
           bool_or(COALESCE(v.hand_verdict, v.verdict) = 'used') AS useful,
           count(*) AS items
      FROM calls c
      CROSS JOIN LATERAL jsonb_array_elements_text(COALESCE(c.hold, '[]'::jsonb)) h(id)
      JOIN memory.supplied_items si
        ON si.session_id = c.session_id
       AND si.item_kind = 'lesson'
       AND si.lesson_id = h.id::bigint
       AND si.supplied_at > c.created_at
      JOIN memory.supplied_item_verdicts v ON v.supplied_item_id = si.id
     GROUP BY c.record_id
),
verdicts AS (
    SELECT * FROM sent
    UNION ALL
    SELECT * FROM held
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT v.record_id,
       CASE
           WHEN v.label_source = 'sent_lesson_verdict' AND v.useful THEN 'right'
           WHEN v.label_source = 'sent_lesson_verdict' THEN 'wrong'
           WHEN v.useful THEN 'wrong'
           ELSE 'right'
       END,
       v.label_source,
       jsonb_build_object('judged_items', v.items, 'any_used', v.useful)
  FROM verdicts v
 WHERE NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = v.record_id AND o.label_source = v.label_source)
ON CONFLICT (record_id, label_source, label) DO NOTHING
