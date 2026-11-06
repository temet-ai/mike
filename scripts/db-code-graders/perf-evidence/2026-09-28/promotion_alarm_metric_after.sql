WITH cfg AS (
    SELECT coalesce(max((value #>> '{}')::bigint), 0) AS live_id
      FROM config.settings
     WHERE key = 'turn_judge.live_bundle_id'
),
latest AS (
    SELECT max(id) AS rollup_id FROM eval.rollups
     WHERE finished_at IS NOT NULL AND started_at <= now()::timestamptz
),
agg AS (
    SELECT r.metric, sum(r.k)::bigint AS k, sum(r.n)::bigint AS n
      FROM eval.rates r, cfg, latest
     WHERE cfg.live_id > 0
       AND r.rollup_id = latest.rollup_id
       AND r.family = 'accuracy' AND r.role = 'turn_judge'
       AND r.axis = 'legacy_outcome' AND r.class = 'corrected'
       AND r.bundle_label = 'bundle:' || cfg.live_id::text
       AND r.label_tier = ANY(ARRAY['gold', 'relayed'])
       AND r.metric IN ('precision', 'recall')
       AND r.window_days = 30
     GROUP BY r.metric
)
SELECT 'turn_judge:' || cfg.live_id::text || ':' || agg.metric AS subject,
       jsonb_build_object(
           'k', agg.k, 'n', agg.n, 'bundle_id', cfg.live_id, 'window_days', 30,
           'wilson_high', eval.wilson_high(agg.k::float8 / nullif(agg.n, 0), agg.n::float8),
           'floor', CASE WHEN agg.metric = 'precision' THEN 0.85 ELSE 0.65 END) AS detail,
       agg.n >= 20
         AND eval.wilson_high(agg.k::float8 / nullif(agg.n, 0), agg.n::float8)
             < CASE WHEN agg.metric = 'precision' THEN 0.85 ELSE 0.65 END AS fires
  FROM agg CROSS JOIN cfg