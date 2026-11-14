UPDATE wright.investigations i
   SET state = 'reconciled', ended_at = now(), end_reason = r.reason
  FROM unnest($1::bigint[], $2::text[]) AS r(id, reason)
 WHERE i.id = r.id AND i.state IN ('starting', 'running')
RETURNING i.id
