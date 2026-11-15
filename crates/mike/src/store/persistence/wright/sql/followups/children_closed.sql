SELECT array_agg(DISTINCT c.issue_number ORDER BY c.issue_number)
  FROM wright.followup_findings root
  JOIN wright.followup_findings ch
    ON ch.finding_id = root.finding_id AND ch.followup_id <> root.followup_id
  JOIN wright.followups c ON c.id = ch.followup_id
 WHERE root.followup_id = $1
HAVING count(*) > 0
   AND bool_and(c.state IN ('fixed', 'dropped'))
   AND bool_and(c.issue_number IS NOT NULL)
