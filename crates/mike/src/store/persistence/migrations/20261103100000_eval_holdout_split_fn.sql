CREATE OR REPLACE FUNCTION eval.holdout_split(turn_id bigint, holdout_percent bigint)
RETURNS text
LANGUAGE sql
IMMUTABLE
PARALLEL SAFE
AS $$
    SELECT CASE WHEN (((hashtextextended(turn_id::text, 0) % 100) + 100) % 100) < holdout_percent
                THEN 'holdout' ELSE 'dev' END
$$;
