INSERT INTO memory.search_tuning_runs
       (ran_at, setting_key, before, after, score_before, score_after, graded_events, kept, reason)
SELECT $9::timestamptz, t.setting_key, t.before, t.after, t.score_before, t.score_after, $8::int,
       t.kept, t.reason
  FROM UNNEST($1::text[], $2::jsonb[], $3::jsonb[], $4::float8[], $5::float8[], $6::bool[], $7::text[])
       AS t(setting_key, before, after, score_before, score_after, kept, reason)
