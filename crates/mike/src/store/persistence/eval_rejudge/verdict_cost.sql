WITH recent AS (
    SELECT v.cost_usd
      FROM memory.turn_verdicts v
     WHERE v.bundle_id = $1 AND v.mode = 'live' AND v.reaction IS NOT NULL
       AND v.cost_usd IS NOT NULL
     ORDER BY v.id DESC
     LIMIT 200
)
SELECT avg(cost_usd)::float8 AS "mean_cost?" FROM recent
