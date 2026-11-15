UPDATE wright.followups
   SET state = 'filed', issue_number = $2::int, issue_url = $3::text,
       filed_at = now(), updated_at = now()
 WHERE id = $1::bigint AND state = 'open'
RETURNING id
