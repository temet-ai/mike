WITH supplies AS (
    SELECT (e.sig ->> 'decision_record_id')::bigint AS record_id,
           ks.session_uuid, ks.thread_id, ks.created_at
      FROM memory.keeper_supplies ks
      CROSS JOIN LATERAL jsonb_array_elements(
               CASE WHEN jsonb_typeof(ks.signals) = 'array' THEN ks.signals ELSE '[]'::jsonb END
           ) e(sig)
     WHERE ks.created_at >= now() - interval '30 days'
       AND ks.arm = 'keeper'
       AND ks.thread_id IS NOT NULL
       AND e.sig ? 'decision_record_id'
),
verdicts AS (
    SELECT s.record_id,
           bool_or(COALESCE(v.hand_verdict, v.verdict) = 'used') AS used,
           count(*) AS cards
      FROM supplies s
      JOIN decisions.records r ON r.id = s.record_id AND r.point = 'thread_pick'
      JOIN sessions.sessions ss ON ss.session_uuid = s.session_uuid
      JOIN memory.supplied_items si
        ON si.session_id = ss.id
       AND si.item_kind = 'card'
       AND si.thread_id = s.thread_id
       AND si.supplied_at BETWEEN s.created_at - interval '1 minute'
                              AND s.created_at + interval '10 minutes'
      JOIN memory.supplied_item_verdicts v ON v.supplied_item_id = si.id
     GROUP BY s.record_id
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT v.record_id,
       CASE WHEN v.used THEN 'right' ELSE 'wrong' END,
       'supplied_card_verdict',
       jsonb_build_object('judged_cards', v.cards, 'any_used', v.used)
  FROM verdicts v
 WHERE NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = v.record_id AND o.label_source = 'supplied_card_verdict')
ON CONFLICT (record_id, label_source, label) DO NOTHING
