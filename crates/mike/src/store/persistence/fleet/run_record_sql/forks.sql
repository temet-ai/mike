SELECT count(*) FILTER (WHERE l.sibling_wait_seconds IS NOT NULL) AS "waited!",
       count(*) AS "forks!"
  FROM launch.launches l
 WHERE l.run_id = $1 AND l.template_id IS NOT NULL
