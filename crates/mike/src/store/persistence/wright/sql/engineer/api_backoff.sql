WITH last AS (
    SELECT id, state, session_uuid, ended_at
      FROM wright.investigations
     WHERE ended_at IS NOT NULL
     ORDER BY id DESC
     LIMIT 1
),
reset AS (
    SELECT max(to_timestamp((h->>1)::double precision)) AS at
      FROM last
      JOIN sessions.sessions s ON s.session_uuid = last.session_uuid
      JOIN requests.requests r ON r.session_id = s.id
      JOIN responses.responses p ON p.request_id = r.id
     CROSS JOIN LATERAL jsonb_array_elements(p.response_headers) h
     WHERE last.state = 'unavailable'
       AND p.response_status = 429
       AND h->>0 = 'anthropic-ratelimit-unified-reset'
       AND (h->>1) ~ '^[0-9]+$'
),
streak AS (
    SELECT count(*) AS refused
      FROM (SELECT state, row_number() OVER (ORDER BY id DESC) AS place
              FROM wright.investigations
             WHERE ended_at IS NOT NULL
             ORDER BY id DESC
             LIMIT 8) recent
     WHERE place < COALESCE((SELECT min(place)
                               FROM (SELECT state, row_number() OVER (ORDER BY id DESC) AS place
                                       FROM wright.investigations
                                      WHERE ended_at IS NOT NULL
                                      ORDER BY id DESC
                                      LIMIT 8) r2
                              WHERE state <> 'unavailable'), 9)
)
SELECT CASE
           WHEN last.state <> 'unavailable' THEN NULL
           WHEN reset.at IS NOT NULL THEN LEAST(reset.at, last.ended_at + interval '6 hours')
           ELSE last.ended_at + LEAST(interval '6 hours', interval '10 minutes' * power(2, streak.refused - 1))
       END AS until
  FROM last CROSS JOIN reset CROSS JOIN streak
