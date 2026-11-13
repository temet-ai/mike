WITH guard AS (
    SELECT id FROM steering.steerers WHERE name = 'turn-guard' ORDER BY id LIMIT 1
),
cand AS (
    SELECT DISTINCT ON (e.operator_turn_id, e.question)
           e.id, e.session_id, e.question, e.mode, e.confidence, e.status, e.outcome_label, e.label_source,
           e.arm, s.project_id
      FROM steering.evaluations e
      JOIN guard g ON g.id = e.steerer_id
      JOIN sessions.sessions s ON s.id = e.session_id
     WHERE e.dispatched_at < $1 AND e.dispatched_at >= $1 - interval '30 days'
       AND COALESCE((e.verdict_json -> 'gates' -> e.question ->> 'open')::boolean, false)
       AND COALESCE((e.verdict_json -> 'questions' -> e.question ->> 'applies')::boolean, false)
       AND e.confidence IS NOT NULL
     ORDER BY e.operator_turn_id, e.question, e.dispatched_at DESC, e.id DESC
),
labelled AS (
    SELECT c.id, c.session_id, c.question, c.mode, c.confidence, c.status, c.outcome_label,
           c.label_source, c.arm, c.project_id,
           CASE WHEN c.label_source = 'operator' THEN 'operator'
                WHEN c.label_source = 'session_reply' THEN 'session_reply'
                WHEN c.mode = 'shadow' AND c.label_source IN ('next_message', 'session_end') THEN 'shadow_auto'
                WHEN c.arm = 'holdback' AND c.label_source IN ('next_message', 'session_end') THEN 'holdback_auto'
           END AS basis,
           ((c.status = 'resolved_correction' AND c.outcome_label = 'right')
            OR (c.status = 'resolved_pass' AND c.outcome_label = 'wrong')) AS right_candidate
      FROM cand c
     WHERE c.outcome_label IN ('right', 'wrong')
),
binned AS (
    SELECT l.question, l.mode, l.confidence, l.label_source, l.project_id, l.basis, l.right_candidate,
           CASE WHEN l.confidence < 0.5 THEN '[0,0.5)'
                ELSE to_char(0.5 + (LEAST(width_bucket(l.confidence, 0.5, 1.0, 10), 10) - 1) * 0.05, 'FM0.00')
           END AS bin
      FROM labelled l
     WHERE l.basis IS NOT NULL
),
parts AS (
    SELECT question, mode, NULL::bigint AS project_id,
           CASE WHEN GROUPING(bin) = 1 THEN 'all' ELSE 'confidence_bin_05' END AS dimension,
           CASE WHEN GROUPING(bin) = 1 THEN 'all' ELSE bin END AS dimension_value,
           count(*) FILTER (WHERE right_candidate) AS k, count(*) AS n,
           count(*) FILTER (WHERE basis = 'operator') AS n_operator,
           count(*) FILTER (WHERE basis = 'shadow_auto') AS n_shadow_auto,
           count(*) FILTER (WHERE basis = 'holdback_auto') AS n_holdback_auto,
           count(*) FILTER (WHERE basis = 'session_reply') AS n_session_reply,
           jsonb_build_object(
               'operator', jsonb_build_object('k', count(*) FILTER (WHERE right_candidate AND label_source = 'operator'),
                                              'n', count(*) FILTER (WHERE label_source = 'operator')),
               'session_reply', jsonb_build_object('k', count(*) FILTER (WHERE right_candidate AND label_source = 'session_reply'),
                                                   'n', count(*) FILTER (WHERE label_source = 'session_reply')),
               'session_end', jsonb_build_object('k', count(*) FILTER (WHERE right_candidate AND label_source = 'session_end'),
                                                 'n', count(*) FILTER (WHERE label_source = 'session_end')),
               'next_message', jsonb_build_object('k', count(*) FILTER (WHERE right_candidate AND label_source = 'next_message'),
                                                  'n', count(*) FILTER (WHERE label_source = 'next_message'))) AS by_source
      FROM binned
     GROUP BY GROUPING SETS ((question, mode, bin), (question, mode))
    UNION ALL
    SELECT question, mode, project_id,
           CASE WHEN GROUPING(bin) = 1 THEN 'all' ELSE 'confidence_bin_05' END,
           CASE WHEN GROUPING(bin) = 1 THEN 'all' ELSE bin END,
           count(*) FILTER (WHERE right_candidate), count(*),
           count(*) FILTER (WHERE basis = 'operator'),
           count(*) FILTER (WHERE basis = 'shadow_auto'),
           count(*) FILTER (WHERE basis = 'holdback_auto'),
           count(*) FILTER (WHERE basis = 'session_reply'),
           jsonb_build_object(
               'operator', jsonb_build_object('k', count(*) FILTER (WHERE right_candidate AND label_source = 'operator'),
                                              'n', count(*) FILTER (WHERE label_source = 'operator')),
               'session_reply', jsonb_build_object('k', count(*) FILTER (WHERE right_candidate AND label_source = 'session_reply'),
                                                   'n', count(*) FILTER (WHERE label_source = 'session_reply')),
               'session_end', jsonb_build_object('k', count(*) FILTER (WHERE right_candidate AND label_source = 'session_end'),
                                                 'n', count(*) FILTER (WHERE label_source = 'session_end')),
               'next_message', jsonb_build_object('k', count(*) FILTER (WHERE right_candidate AND label_source = 'next_message'),
                                                  'n', count(*) FILTER (WHERE label_source = 'next_message')))
      FROM binned
     WHERE project_id IS NOT NULL
     GROUP BY GROUPING SETS ((question, mode, project_id, bin), (question, mode, project_id))
)
INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind, project_id, window_start,
                        window_end, window_days, role, axis, class, label_split, dimension, dimension_value,
                        steerer_id, question, mode, k, n, value, ci_low, ci_high, weighted, detail)
SELECT $2, 'steer_calibration', 'precision', 'steerer_evaluations',
       CASE WHEN p.project_id IS NULL THEN 'global' ELSE 'project' END, p.project_id,
       $1 - interval '30 days', $1, 30, 'steerer', 'steer_candidate', 'applies', 'dev',
       p.dimension, p.dimension_value, g.id, p.question, p.mode, p.k::int, p.n::int,
       p.k::float8 / nullif(p.n, 0),
       eval.wilson_low(p.k::float8 / nullif(p.n, 0), p.n),
       eval.wilson_high(p.k::float8 / nullif(p.n, 0), p.n), false,
       jsonb_build_object('label_basis', jsonb_build_object('operator', p.n_operator,
                          'shadow_auto', p.n_shadow_auto, 'holdback_auto', p.n_holdback_auto,
                          'session_reply', p.n_session_reply),
                          'by_source', p.by_source)
  FROM parts p
  CROSS JOIN guard g
