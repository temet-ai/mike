SELECT count(*) AS "total!" FROM wright.problems p
 WHERE ($1::text IS NULL OR p.state = $1)
   AND ($2::text IS NULL OR p.check_name = $2)
   AND ($3::text IS NULL OR p.source = $3)
