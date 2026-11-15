SELECT f.id, f.kind, f.check_name, f.owning_part, f.title, f.body, f.state, f.issue_number, f.issue_url,
       f.attempts, f.fixer_session, f.fixer_machine, f.filed_at, f.handed_off_at, f.closed_at,
       f.closed_reason, f.created_at, f.updated_at
  FROM wright.followups f
 WHERE f.state = 'filed'
   AND NOT (f.kind = 'code_defect' AND f.id IN (
       SELECT ff.followup_id
         FROM wright.followup_findings ff
         JOIN wright.repairs r ON r.finding_id = ff.finding_id))
 ORDER BY f.filed_at, f.id
 LIMIT 1
