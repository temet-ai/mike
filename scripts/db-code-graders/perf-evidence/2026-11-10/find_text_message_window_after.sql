SELECT d.request_id, d.ord, m.body::text
FROM requests.deltas d
JOIN messages.messages m ON m.id = d.message_id
WHERE d.session_id = 2111366
  AND d.request_id = ANY(ARRAY[760265,760266,760269,760271,760272,760275,760276,760280,760282,760284,760286,760289,760295,760297,760300,760306,760308,760309,760310,760311,760312,760315,760317,760318,760321,760322,760343,760346,760350,760351,760352,760353,760357,760358,760359,760360,760361,760362,760363,760364,760365,760369,760370,760371,760374,760377,760379,760380,760381,760382]::bigint[])
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
