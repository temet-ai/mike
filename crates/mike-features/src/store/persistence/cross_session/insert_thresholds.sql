WITH ins AS (
    INSERT INTO steering.guard_thresholds
        (steerer_id, question, scope, project_id, rollup_id, rollup_finished_at, window_start, window_end,
         label_basis, question_mode, threshold_mode, n_labelled, precision_bar, threshold_floor,
         threshold_ceiling, static_threshold, candidate_threshold, applied_threshold, decision, detail)
    SELECT * FROM unnest($1::bigint[], $2::text[], $3::text[], $4::bigint[], $5::bigint[],
                         $6::timestamptz[], $7::timestamptz[], $8::timestamptz[], $9::text[], $10::text[],
                         $11::text[], $12::int[], $13::float8[], $14::real[], $15::real[], $16::real[],
                         $17::real[], $18::real[], $19::text[], $20::jsonb[])
    RETURNING id, steerer_id, question, scope, project_id, applied_threshold, decision, detail, rollup_id
)
INSERT INTO config.change_log (subject_kind, subject_key, op, old_value, new_value, via, detail)
SELECT 'guard_threshold',
       steerer_id || ':' || question || ':' || scope || ':' || coalesce(project_id::text, 'global'),
       'update',
       jsonb_build_object('threshold', (detail ->> 'previous_threshold')::real),
       jsonb_build_object('threshold', applied_threshold),
       'calibration',
       jsonb_build_object('threshold_row_id', id, 'rollup_id', rollup_id)
  FROM ins WHERE decision = 'applied'
RETURNING id AS "id!"
