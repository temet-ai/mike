UPDATE wright.followups
   SET state = 'handed_off', fixer_session = $2::uuid, fixer_machine = $3::text,
       handed_off_at = now(), updated_at = now()
 WHERE id = $1::bigint AND state = 'filed'
RETURNING id
