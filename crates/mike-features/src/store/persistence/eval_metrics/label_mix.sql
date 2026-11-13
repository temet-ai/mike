WITH days AS (
    SELECT unnest($1::timestamptz[]) AS d),
bounds AS (
    SELECT min(d) - interval '35 days' AS lo, max(d) AS hi FROM days),
j AS (
    SELECT p.created_at, p.legacy_outcome AS code, p.bundle_label, p.bundle_id, p.is_replied
      FROM eval.turn_population_v p
      JOIN bounds ON p.created_at >= bounds.lo AND p.created_at < bounds.hi
     WHERE p.is_real AND p.legacy_outcome <> 'unknown'
),
wj AS (
    SELECT dy.d,
           CASE WHEN j.created_at >= dy.d - interval '7 days' THEN 'current' ELSE 'baseline' END AS win,
           j.code, j.bundle_label, j.bundle_id, j.is_replied
      FROM days dy
      JOIN j ON j.created_at >= dy.d - interval '35 days' AND j.created_at < dy.d
),
cls AS (
    SELECT d, win, code, count(*) AS k, count(*) FILTER (WHERE is_replied) AS kr
      FROM wj GROUP BY d, win, code
),
per_win AS (
    SELECT d, win, jsonb_object_agg(code, k) AS classes, sum(k)::bigint AS n,
           sum(kr)::bigint AS replied_n,
           coalesce(sum(kr) FILTER (WHERE code = 'corrected'), 0)::bigint AS replied_corrected
      FROM cls GROUP BY d, win
),
bun AS (
    SELECT wj.d, wj.win, wj.bundle_label, wj.bundle_id, count(*) AS k,
           coalesce(bool_or(b.status = 'live' OR b.gate_run_id IS NOT NULL
                            OR b.override_reason IS NOT NULL), false) AS gated
      FROM wj LEFT JOIN eval.bundles b ON b.id = wj.bundle_id
     GROUP BY wj.d, wj.win, wj.bundle_label, wj.bundle_id
),
per_bun AS (
    SELECT d, win, jsonb_agg(jsonb_build_object('label', bundle_label, 'bundle_id', bundle_id,
                                                'count', k, 'gated', gated) ORDER BY k DESC) AS bundles
      FROM bun GROUP BY d, win
)
SELECT pw.d AS "d!", pw.win AS "win!", pw.n AS "n!", pw.classes AS "classes!", pb.bundles AS "bundles!",
       pw.replied_n AS "replied_n!", pw.replied_corrected AS "replied_corrected!"
  FROM per_win pw JOIN per_bun pb USING (d, win)
 ORDER BY pw.d, pw.win
