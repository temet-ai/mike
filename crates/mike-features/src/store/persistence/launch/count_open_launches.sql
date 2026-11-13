SELECT count(*) AS "count!"
  FROM launch.launches
 WHERE backend = $1
   AND closed_at IS NULL
   AND opened
