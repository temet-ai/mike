INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind,
        project_id, window_start, window_end, window_days, role, bundle_label, axis, class,
        label_tier, label_split, label_source, dimension, dimension_value, steerer_id, question,
        mode, k, n, value, ci_low, ci_high, weighted, n_effective, detail)
    SELECT $1, x.family, x.metric, x.population, x.scope_kind, x.project_id, x.window_start,
           x.window_end, x.window_days, x.role, x.bundle_label, x.axis, x.class, x.label_tier,
           x.label_split, x.label_source, x.dimension, x.dimension_value, x.steerer_id, x.question,
           x.mode, x.k, x.n, x.k::float8 / nullif(x.n, 0),
           eval.wilson_low(x.k::float8 / nullif(x.n, 0), x.n),
           eval.wilson_high(x.k::float8 / nullif(x.n, 0), x.n), false, NULL,
           coalesce(x.detail, '{}'::jsonb)
      FROM jsonb_to_recordset($2::jsonb) AS x(
           family text, metric text, population text, scope_kind text, project_id bigint,
           window_start timestamptz, window_end timestamptz, window_days integer, role text,
           bundle_label text, axis text, class text, label_tier text, label_split text,
           label_source text, dimension text, dimension_value text, steerer_id bigint,
           question text, mode text, k integer, n integer, detail jsonb)
