SELECT count(*) FILTER (WHERE l.sibling_wait_seconds IS NOT NULL) AS waited,
       count(*) AS forks
  FROM launch.launches l
 WHERE l.run_id = 'fleet-change-record-3-30a0427e' AND l.template_id IS NOT NULL
