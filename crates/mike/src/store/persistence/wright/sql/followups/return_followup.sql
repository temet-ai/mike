UPDATE wright.followups
   SET attempts = attempts + 1,
       state = CASE WHEN attempts + 1 >= $2::int THEN 'stuck' ELSE 'filed' END,
       fixer_session = NULL, fixer_machine = NULL, updated_at = now()
 WHERE id = $1::bigint AND state = 'handed_off'
RETURNING state
