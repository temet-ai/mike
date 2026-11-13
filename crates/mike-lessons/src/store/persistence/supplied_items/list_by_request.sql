SELECT i.id, i.request_id, s.session_uuid AS session_id, i.item_kind, i.lesson_id, i.thread_id,
       t.slug AS "thread_slug?", i.card_hash, i.route, i.score, i.chance, i.item_text,
       i.supplied_at, v.verdict AS "verdict?", v.rationale AS "rationale?",
       v.evidence AS "evidence?", v.judged_at AS "judged_at?", v.hand_verdict
  FROM memory.supplied_items i
  JOIN sessions.sessions s ON s.id = i.session_id
  LEFT JOIN work.threads t ON t.id = i.thread_id
  LEFT JOIN memory.supplied_item_verdicts v ON v.supplied_item_id = i.id
 WHERE i.request_id = $1
 ORDER BY i.supplied_at, i.id
 LIMIT $2 OFFSET $3
