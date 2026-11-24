SELECT EXISTS (SELECT 1 FROM wright.causes c WHERE c.id = $1::bigint)
