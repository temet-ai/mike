SELECT d.request_id, d.ord, m.body::text
FROM requests.deltas d
JOIN messages.messages m ON m.id = d.message_id
WHERE d.session_id = 2111366
  AND d.request_id = ANY(ARRAY[757098,757099,757100,757101,757102,757103,757104,757105,757106,757107,757108,757109,757110,757111,757112,757113,757114,757115,757116,757117,757118,757119,757120,757121,757122,757123,757124,757125,757126,757127,757128,757129,757130,757131,757132,757133,757134,757135,757136,757137,757138,757139,757140,757141,757142,757143,757144,757145,757146,757147]::bigint[])
  AND d.change_kind_id = 1
  AND (CASE WHEN true THEN strpos(m.body::text, 'b013f3306')
            ELSE strpos(lower(m.body::text), lower('b013f3306')) END) > 0
  AND NOT EXISTS (
        SELECT 1
        FROM requests.deltas e
        WHERE e.session_id = d.session_id
          AND e.message_id = d.message_id
          AND e.change_kind_id = 1
          AND (e.request_id, e.ord) < (d.request_id, d.ord)
      )
ORDER BY d.request_id, d.ord
