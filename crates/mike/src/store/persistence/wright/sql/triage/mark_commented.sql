UPDATE wright.causes SET commented_at = now() WHERE id = $1::bigint
