WITH present AS (
    SELECT EXISTS (SELECT 1 FROM eval.bundles) AS any_bundle
),
tj AS (
    SELECT count(*) AS n, min(classified_at) AS first_at, max(classified_at) AS last_at
      FROM memory.operator_turns
     WHERE judge_model IS NOT NULL AND outcome_bundle_id IS NULL
       AND classified_at >= now() - interval '24 hours'
),
sj AS (
    SELECT count(*) AS n, min(judged_at) AS first_at, max(judged_at) AS last_at
      FROM session_judges.verdicts
     WHERE bundle_id IS NULL AND mode = 'live' AND judged_at >= now() - interval '24 hours'
)
SELECT 'unstamped:turn_judge' AS "subject!",
       jsonb_build_object('count', tj.n, 'first_at', tj.first_at, 'last_at', tj.last_at) AS "detail!"
  FROM tj CROSS JOIN present
 WHERE present.any_bundle AND tj.n > 0 AND 'turn_judge' = ANY($1::text[])
UNION ALL
SELECT 'unstamped:session_judge',
       jsonb_build_object('count', sj.n, 'first_at', sj.first_at, 'last_at', sj.last_at)
  FROM sj CROSS JOIN present
 WHERE present.any_bundle AND sj.n > 0 AND 'session_judge' = ANY($1::text[])
