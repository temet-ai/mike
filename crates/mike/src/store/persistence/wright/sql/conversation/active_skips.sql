SELECT DISTINCT ON ((change->>'id')::bigint)
       (change->>'id')::bigint AS problem_id, (change->>'until')::timestamptz AS until
  FROM wright.steers
 WHERE applied_at IS NOT NULL AND undone_at IS NULL
   AND change->>'kind' = 'skip_problem'
   AND (change->>'until')::timestamptz > now()
 ORDER BY (change->>'id')::bigint, (change->>'until')::timestamptz DESC
