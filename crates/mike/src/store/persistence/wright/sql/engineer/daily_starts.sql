SELECT (SELECT count(*)
          FROM wright.investigations
         WHERE created_at >= date_trunc('day', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC') AS started,
       (SELECT daily_starts FROM wright.engineer_limits WHERE id) AS daily_limit
