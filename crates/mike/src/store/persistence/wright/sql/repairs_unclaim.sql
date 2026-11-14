DELETE FROM wright.repairs WHERE id = $1 AND state = 'editing' RETURNING id
