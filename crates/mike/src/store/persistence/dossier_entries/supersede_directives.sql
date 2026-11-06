UPDATE memory.dossier_entries d
    SET status = 'superseded',
        closed_at = n.effective_at,
        close_recorded_at = now(),
        closed_by = n.source_ref
   FROM (
        SELECT DISTINCT ON (d.id) d.id, n.effective_at, n.source_ref
          FROM memory.dossier_entries d
          JOIN memory.dossier_entries n
            ON n.session_id = d.session_id
           AND n.kind = 'directive'
           AND n.data->>'class' = d.data->>'class'
           AND n.data->>'polarity' <> d.data->>'polarity'
           AND n.effective_at > d.effective_at
         WHERE d.kind = 'directive'
           AND d.status = 'open'
           AND d.session_id = ANY($1)
         ORDER BY d.id, n.effective_at, n.id
   ) n
  WHERE d.id = n.id
    AND d.status = 'open'
