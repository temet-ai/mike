WITH reaped AS (
    UPDATE workspace.session_registrations r
       SET released_at = now(),
           reaped_at   = now()
     WHERE r.session_id = ANY(ARRAY[398402,480892,478098]::bigint[])
       AND r.released_at IS NULL
 RETURNING r.session_id, r.project_id
),
ledger AS (
    INSERT INTO workspace.coordination_events
        (project_id, session_id, actor_label, kind_id, subject, detail)
    SELECT reaped.project_id, reaped.session_id, 'workspace-reaper',
           ek.id, s.session_uuid::text, '{}'::jsonb
      FROM reaped
      JOIN sessions.sessions s ON s.id = reaped.session_id
      CROSS JOIN workspace.event_kinds ek
     WHERE ek.code = 'reaped'
    RETURNING 1
)
SELECT count(*) FROM reaped;
