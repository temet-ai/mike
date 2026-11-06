WITH days AS (
          SELECT ((now() AT TIME ZONE 'UTC')::date - offset_days)::date AS day
          FROM generate_series(0,6) AS offset_days
          ), calls AS (
          SELECT (created_at AT TIME ZONE 'UTC')::date AS day,count(*) AS actual_calls
          FROM llm.calls WHERE role='lesson_proposer'
          AND created_at >= (((now() AT TIME ZONE 'UTC')::date - 6)::timestamp AT TIME ZONE 'UTC')
          AND created_at < (((now() AT TIME ZONE 'UTC')::date + 1)::timestamp AT TIME ZONE 'UTC')
          GROUP BY 1)
          SELECT d.day,coalesce(b.consumed,0) AS consumed,coalesce(c.actual_calls,0) AS recorded_calls
          FROM days d LEFT JOIN memory.lesson_proposer_daily_budget b ON b.day=d.day
          LEFT JOIN calls c ON c.day=d.day ORDER BY d.day;
