WITH fires AS (
    SELECT i.id, i.session_id, i.operator_turn_id AS stamped, r.operator_turn_id AS request_turn,
           r.created_at AS request_at
      FROM steering.interventions i
      JOIN steering.steerers st ON st.id = i.steerer_id AND st.kind = 'verifier'
      JOIN steering.verdicts v ON v.id = i.verdict_id AND v.code = 'fired'
      LEFT JOIN requests.requests r ON r.id = i.trigger_request_id
     WHERE i.created_at < $1
),
prior AS (
    SELECT DISTINCT ON (f.id) f.id, t.id AS turn_id
      FROM fires f
      JOIN memory.operator_turns t ON t.session_id = f.session_id AND t.created_at <= f.request_at
     WHERE f.stamped IS NULL AND f.request_turn IS NULL
     ORDER BY f.id, t.created_at DESC
),
fired_turns AS (
    SELECT DISTINCT coalesce(f.stamped, f.request_turn, prior.turn_id) AS turn_id
      FROM fires f LEFT JOIN prior ON prior.id = f.id
),
census AS (
    SELECT l.operator_turn_id, l.label,
           l.tags && ARRAY['false-done', 'unverified-claim'] AS tagged
      FROM eval.labels_as_of($3) l
     WHERE l.subject_kind = 'turn' AND l.axis = 'reaction' AND l.source = 'fleet_mirror/census'
),
joined AS (
    SELECT c.*, ft.turn_id IS NOT NULL AS fired
      FROM census c LEFT JOIN fired_turns ft ON ft.turn_id = c.operator_turn_id
),
agg AS (
    SELECT 'fire_precision'::text AS metric, NULL::text AS dimension, NULL::text AS dimension_value,
           count(*) FILTER (WHERE fired AND label = 'corrected') AS k,
           count(*) FILTER (WHERE fired) AS n
      FROM joined
    UNION ALL
    SELECT 'fire_precision', 'failure_tag', 'false-done|unverified-claim',
           count(*) FILTER (WHERE fired AND label = 'corrected' AND tagged),
           count(*) FILTER (WHERE fired)
      FROM joined
    UNION ALL
    SELECT 'fire_recall', 'failure_tag', 'false-done|unverified-claim',
           count(*) FILTER (WHERE fired AND label = 'corrected' AND tagged),
           count(*) FILTER (WHERE label = 'corrected' AND tagged)
      FROM joined
)
INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind, window_end, role,
                        axis, class, label_source, dimension, dimension_value, mode, k, n, value,
                        ci_low, ci_high, weighted, detail)
SELECT $2, 'steer_calibration', agg.metric, 'labelled_turns', 'global', $1, 'steerer',
       'reaction', 'corrected', 'fleet_mirror/census', agg.dimension, agg.dimension_value, 'live',
       agg.k::int, agg.n::int, agg.k::float8 / nullif(agg.n, 0),
       eval.wilson_low(agg.k::float8 / nullif(agg.n, 0), agg.n),
       eval.wilson_high(agg.k::float8 / nullif(agg.n, 0), agg.n), false,
       jsonb_build_object('baseline', 'verifier')
  FROM agg
