WITH recent AS (
    SELECT state, ended_at, row_number() OVER (ORDER BY id DESC) AS place
      FROM wright.investigations
     WHERE ended_at IS NOT NULL
     ORDER BY id DESC
     LIMIT 8
),
streak AS (
    SELECT count(*) AS refused
      FROM recent
     WHERE place < COALESCE((SELECT min(place) FROM recent WHERE state <> 'unavailable'), 9)
)
SELECT EXISTS (
    SELECT 1
      FROM recent r CROSS JOIN streak s
     WHERE r.place = 1
       AND r.state = 'unavailable'
       AND r.ended_at > now() - LEAST(interval '6 hours', interval '10 minutes' * power(2, s.refused - 1))
)
