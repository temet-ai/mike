SELECT EXISTS (SELECT 1 FROM wright.cause_groups g WHERE g.id = $1::bigint)
