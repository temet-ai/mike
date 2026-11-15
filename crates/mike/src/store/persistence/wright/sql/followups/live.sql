SELECT f.id, f.kind, f.check_name, f.owning_part, f.title, f.body, f.state, f.issue_number, f.issue_url,
       f.attempts, f.fixer_session, f.fixer_machine, f.filed_at, f.handed_off_at, f.closed_at,
       f.closed_reason, f.created_at, f.updated_at
  FROM wright.followups f
 WHERE f.state IN ('open', 'filed', 'handed_off', 'stuck')
   AND (f.check_name = $1::text OR f.owning_part = $2::text)
 ORDER BY f.id DESC
 LIMIT 20
