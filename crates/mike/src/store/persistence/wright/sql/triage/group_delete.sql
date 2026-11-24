DELETE FROM wright.cause_groups g
 WHERE g.id = $1::bigint
RETURNING g.id
