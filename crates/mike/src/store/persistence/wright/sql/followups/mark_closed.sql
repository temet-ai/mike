UPDATE wright.followups
   SET state = $2::text, closed_at = now(), closed_reason = $3::text, updated_at = now()
 WHERE id = $1::bigint
   AND $2::text IN ('fixed', 'dropped')
   AND state IN ('open', 'filed', 'handed_off', 'stuck')
RETURNING id
