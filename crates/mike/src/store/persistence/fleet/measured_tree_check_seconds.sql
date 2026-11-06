SELECT percentile_cont(0.5) WITHIN GROUP (ORDER BY seconds)::float8
FROM (
    SELECT EXTRACT(EPOCH FROM finished_at - executing_at)::float8 AS seconds
    FROM launch.tree_checks
    WHERE btrim(command) = $1
      AND executing_at IS NOT NULL
      AND finished_at IS NOT NULL
      AND exit_code IS NOT NULL
    ORDER BY id DESC
    LIMIT 20
) recent
