SELECT v.supplied_item_id, i.request_id, i.item_kind, i.route, i.item_text, v.verdict,
       v.rationale, v.evidence, v.input_text, v.judged_at, v.hand_verdict,
       v.hand_checked_by, v.hand_checked_at
  FROM memory.supplied_item_verdicts v
  JOIN memory.supplied_items i ON i.id = v.supplied_item_id
 WHERE v.judged_at >= $1
   AND v.judged_at < $1 + interval '7 days'
 ORDER BY md5(v.supplied_item_id::text || $1::text), v.supplied_item_id
 LIMIT $2
