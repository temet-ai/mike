WITH cm AS (
    SELECT production_axis, production_class, label_axis, label_value, replied_counts_as_negative
      FROM eval.class_map
     WHERE production_axis IN ('legacy_outcome', 'legacy_session_outcome')
),
lab AS (
    SELECT l.operator_turn_id, l.axis, l.label, l.tier, l.split, l.source, l.inclusion_prob
      FROM eval.labels_as_of($1) l
     WHERE l.subject_kind IN ('turn', 'session_end')
),
pop AS (
    SELECT p.operator_turn_id, p.session_id, p.is_replied, p.legacy_outcome, p.bundle_label,
           p.writer_role
      FROM eval.turn_population_v p
     WHERE p.is_real AND p.created_at < $2 AND p.created_at >= COALESCE($3::timestamptz, '-infinity'::timestamptz)
),
sv AS (
    SELECT DISTINCT ON (v.session_id) v.session_id, so.code, v.judge_model, v.rubric_version
      FROM session_judges.verdicts v
      JOIN session_judges.outcomes so ON so.id = v.outcome_id
     WHERE v.judged_at < $2
       AND v.mode = 'live'
     ORDER BY v.session_id, v.judged_at DESC, v.id DESC
),
prod AS (
    SELECT p.operator_turn_id, p.is_replied, 'legacy_outcome'::text AS production_axis,
           p.legacy_outcome AS verdict, p.bundle_label, p.writer_role, 'turn_judge'::text AS role
      FROM pop p
    UNION ALL
    SELECT p.operator_turn_id, p.is_replied, 'legacy_session_outcome', sv.code,
           'legacy:' || sv.judge_model || ':' || sv.rubric_version::text, 'session_judge', 'session_judge'
      FROM pop p JOIN sv ON sv.session_id = p.session_id
),
slices AS (
    SELECT DISTINCT axis, tier, split, source FROM lab),
scored0 AS (
    SELECT pr.role, pr.production_axis, pr.bundle_label, pr.writer_role, cm.production_class AS class,
           lab.tier, lab.split, lab.source, lab.inclusion_prob, true AS from_label,
           pr.verdict = cm.production_class AS predicted, lab.label = cm.label_value AS positive
      FROM prod pr
      JOIN cm ON cm.production_axis = pr.production_axis
      JOIN lab ON lab.operator_turn_id = pr.operator_turn_id AND lab.axis = cm.label_axis
    UNION ALL
    SELECT pr.role, pr.production_axis, pr.bundle_label, pr.writer_role, cm.production_class,
           sl.tier, sl.split, sl.source, NULL::float8, false,
           pr.verdict = cm.production_class, false
      FROM prod pr
      JOIN cm ON cm.production_axis = pr.production_axis AND cm.replied_counts_as_negative
      JOIN slices sl ON sl.axis = cm.label_axis
     WHERE pr.is_replied
       AND NOT EXISTS (SELECT 1 FROM lab WHERE lab.operator_turn_id = pr.operator_turn_id
                                           AND lab.axis = cm.label_axis)
),
sw AS (
    SELECT tier, split, source, bool_or(from_label AND inclusion_prob IS NOT NULL) AS weighted
      FROM scored0 GROUP BY tier, split, source
),
scored AS (
    SELECT s.role, s.production_axis, s.bundle_label, s.writer_role, s.class, s.tier, s.split,
           s.source, s.inclusion_prob, s.from_label, s.predicted, s.positive, sw.weighted,
           (sw.weighted AND s.from_label AND s.inclusion_prob IS NULL) AS dropped,
           CASE WHEN s.from_label AND s.inclusion_prob IS NOT NULL THEN 1.0 / s.inclusion_prob
                ELSE 1.0 END AS w
      FROM scored0 s JOIN sw USING (tier, split, source)
),
agg AS (
    SELECT role, production_axis, class, tier, split, source, bundle_label, writer_role,
           GROUPING(bundle_label, writer_role) AS grp,
           count(*) FILTER (WHERE predicted AND positive AND NOT dropped) AS tp_n,
           count(*) FILTER (WHERE predicted AND NOT dropped) AS pred_n,
           count(*) FILTER (WHERE positive AND NOT dropped) AS pos_n,
           count(*) FILTER (WHERE dropped) AS excluded,
           coalesce(sum(w) FILTER (WHERE predicted AND positive AND NOT dropped), 0) AS tp_w,
           coalesce(sum(w) FILTER (WHERE predicted AND NOT dropped), 0) AS pred_w,
           coalesce(sum(w * w) FILTER (WHERE predicted AND NOT dropped), 0) AS pred_w2,
           coalesce(sum(w) FILTER (WHERE positive AND NOT dropped), 0) AS pos_w,
           coalesce(sum(w * w) FILTER (WHERE positive AND NOT dropped), 0) AS pos_w2,
           bool_or(weighted) AS weighted
      FROM scored
     GROUP BY GROUPING SETS ((role, production_axis, class, tier, split, source, bundle_label),
                             (role, production_axis, class, tier, split, source, writer_role),
                             (role, production_axis, class, tier, split, source))
),
m AS (
    SELECT agg.role, agg.production_axis, agg.class, agg.tier, agg.split, agg.source,
           agg.bundle_label, agg.writer_role, agg.grp, agg.tp_w, agg.pred_w, agg.pos_w, agg.excluded,
           agg.weighted, 'precision'::text AS metric, tp_n AS k, pred_n AS n,
           tp_w / nullif(pred_w, 0) AS value,
           CASE WHEN weighted THEN pred_w ^ 2 / nullif(pred_w2, 0) ELSE pred_n::float8 END AS n_eff
      FROM agg
    UNION ALL
    SELECT agg.role, agg.production_axis, agg.class, agg.tier, agg.split, agg.source,
           agg.bundle_label, agg.writer_role, agg.grp, agg.tp_w, agg.pred_w, agg.pos_w, agg.excluded,
           agg.weighted, 'recall', tp_n, pos_n, tp_w / nullif(pos_w, 0),
           CASE WHEN weighted THEN pos_w ^ 2 / nullif(pos_w2, 0) ELSE pos_n::float8 END
      FROM agg
)
INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind, window_start, window_end,
                        window_days, role, bundle_label, axis, class, label_tier, label_split,
                        label_source, dimension, dimension_value, k, n, value, ci_low, ci_high,
                        weighted, n_effective, detail)
SELECT $4, 'accuracy', m.metric, 'real_turns', 'global', $3, $2, $5, m.role,
       CASE WHEN m.grp = 1 THEN m.bundle_label END,
       m.production_axis, m.class, m.tier, m.split, m.source,
       CASE WHEN m.grp = 2 THEN 'writer_role' END,
       CASE WHEN m.grp = 2 THEN coalesce(m.writer_role, 'none') END,
       m.k::int, m.n::int, m.value,
       eval.wilson_low(m.value, m.n_eff), eval.wilson_high(m.value, m.n_eff),
       m.weighted, CASE WHEN m.weighted THEN m.n_eff END,
       jsonb_build_object('tp_w', m.tp_w, 'pred_w', m.pred_w, 'pos_w', m.pos_w,
                          'excluded_null_weight', m.excluded)
  FROM m
