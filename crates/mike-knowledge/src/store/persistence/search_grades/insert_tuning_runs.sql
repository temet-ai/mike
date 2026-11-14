WITH pass AS (
    INSERT INTO memory.search_tuning_passes
           (ran_at, outcome, graded_events, settings_tried, settings_changed)
    VALUES ($9::timestamptz, $10::text, $8::int, cardinality($1::text[]),
            (SELECT count(*)::int FROM UNNEST($6::bool[]) AS k(kept) WHERE k.kept))
    RETURNING id
)
INSERT INTO memory.search_tuning_runs
       (ran_at, setting_key, before, after, score_before, score_after, graded_events, kept, reason, pass_id)
SELECT $9::timestamptz, t.setting_key, t.before, t.after, t.score_before, t.score_after, $8::int,
       t.kept, t.reason, pass.id
  FROM pass,
       UNNEST($1::text[], $2::jsonb[], $3::jsonb[], $4::float8[], $5::float8[], $6::bool[], $7::text[])
       AS t(setting_key, before, after, score_before, score_after, kept, reason)
