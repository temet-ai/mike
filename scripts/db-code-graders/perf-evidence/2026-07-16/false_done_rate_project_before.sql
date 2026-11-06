SELECT
    project_path     AS "project_path!",
    fires_count      AS "fires_count!: i64",
    corrected_count  AS "corrected_count!: i64",
    delivered_count  AS "delivered_count!: i64",
    corrected_rate,
    delivered_rate
FROM steering.false_done_rate_v
ORDER BY fires_count DESC;
