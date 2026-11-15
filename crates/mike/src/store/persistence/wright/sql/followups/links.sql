SELECT ff.followup_id,
       array_agg(ff.finding_id ORDER BY ff.finding_id) AS findings,
       COALESCE(array_agg(DISTINCT fi.problem_id ORDER BY fi.problem_id)
                    FILTER (WHERE fi.problem_id IS NOT NULL), '{}') AS problems
  FROM wright.followup_findings ff
  JOIN wright.findings fi ON fi.id = ff.finding_id
 WHERE ff.followup_id = ANY($1::bigint[])
 GROUP BY ff.followup_id
