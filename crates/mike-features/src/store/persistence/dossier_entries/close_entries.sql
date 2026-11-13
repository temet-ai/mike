UPDATE memory.dossier_entries d
    SET status = x.status,
        closed_at = x.closed_at,
        close_recorded_at = now(),
        closed_by = x.closed_by
   FROM jsonb_to_recordset($1::jsonb) AS x(
        id bigint, status text, closed_at timestamptz, closed_by text)
  WHERE d.id = x.id
    AND d.status = 'open'
