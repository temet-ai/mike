SELECT f.id, f.kind, f.check_name, f.owning_part, f.title, f.body, f.state, f.issue_number, f.issue_url,
       f.attempts, f.fixer_session, f.fixer_machine, f.filed_at, f.handed_off_at, f.closed_at,
       f.closed_reason, f.created_at, f.updated_at
  FROM wright.followups f
 WHERE ($1::text IS NULL OR f.state = $1::text)
   AND ($3::bigint IS NULL OR f.id < $3::bigint)
 ORDER BY f.id DESC
 LIMIT $2::bigint
