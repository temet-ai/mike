SELECT (SELECT count(*)
          FROM wright.investigations
         WHERE created_at >= date_trunc('day', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
           AND (state IN ('starting', 'running', 'killed') OR tokens > 0 OR tool_calls > 0)) AS started,
       (SELECT daily_starts FROM wright.engineer_limits WHERE id) AS daily_limit
