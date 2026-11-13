INSERT INTO memory.supplied_items
       (session_id, request_id, item_kind, thread_id, card_hash, route, score, chance, item_text)
SELECT s.id, $1, 'card', k.thread_id, c.hash,
       CASE WHEN c.proxy THEN 'proxy' ELSE k.carrier END,
       k.score, k.chance, k.card_text
  FROM UNNEST($2::text[], $3::bool[]) AS c(hash, proxy)
  JOIN sessions.sessions s ON s.session_uuid = $4
  JOIN LATERAL (
       SELECT ks.thread_id, ks.carrier, ks.score, ks.chance, ks.card_text
         FROM memory.keeper_supplies ks
        WHERE ks.session_uuid = $4
          AND ks.block_hash = c.hash
          AND ks.card_text IS NOT NULL
        ORDER BY ks.created_at DESC, ks.id DESC
        LIMIT 1) k ON true
 WHERE NOT EXISTS (
       SELECT 1 FROM memory.supplied_items seen
        WHERE seen.session_id = s.id
          AND seen.card_hash = c.hash
          AND (seen.request_id = $1 OR (NOT c.proxy AND seen.route <> 'proxy')))
