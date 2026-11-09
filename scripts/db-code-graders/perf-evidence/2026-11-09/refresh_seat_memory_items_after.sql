WITH seat_projects AS (
    SELECT DISTINCT s.project_id
      FROM launch.launches l
      JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
     WHERE s.project_id IS NOT NULL
), fresh AS MATERIALIZED (
    SELECT DISTINCT ON (sm.project_id, sm.item_kind, sm.source_ref, sm.session_uuid) sm.*
      FROM seat_projects p
     CROSS JOIN LATERAL memory.seat_memory(p.project_id) sm
     WHERE sm.session_uuid IS NOT NULL AND sm.source_ref IS NOT NULL AND sm.text IS NOT NULL
     ORDER BY sm.project_id, sm.item_kind, sm.source_ref, sm.session_uuid, sm.at
), changed AS (
    SELECT f.*
      FROM fresh f
      LEFT JOIN memory.seat_memory_items i
        ON i.project_id = f.project_id AND i.item_kind = f.item_kind
       AND i.source_ref = f.source_ref AND i.session_uuid = f.session_uuid
     WHERE i.project_id IS NULL
        OR (i.session_id, i.run_id, i.seat, i.task, i.author, i.from_session, i.from_role, i.at,
            i.text, i.repeat_count, i.report_filed, i.verdict_accepted)
           IS DISTINCT FROM
           (f.session_id, f.run_id, f.seat, f.task, f.author, f.from_session, f.from_role, f.at,
            f.text, f.repeat_count, f.report_filed, f.verdict_accepted)
), written AS (
    INSERT INTO memory.seat_memory_items AS i
        (project_id, item_kind, source_ref, session_uuid, session_id, run_id, seat, task, author,
         from_session, from_role, at, text, repeat_count, report_filed, verdict_accepted)
    SELECT f.project_id, f.item_kind, f.source_ref, f.session_uuid, f.session_id, f.run_id, f.seat,
           f.task, f.author, f.from_session, f.from_role, f.at, f.text, f.repeat_count,
           f.report_filed, f.verdict_accepted
      FROM changed f
    ON CONFLICT (project_id, item_kind, source_ref, session_uuid) DO UPDATE
       SET session_id = EXCLUDED.session_id, run_id = EXCLUDED.run_id, seat = EXCLUDED.seat,
           task = EXCLUDED.task, author = EXCLUDED.author, from_session = EXCLUDED.from_session,
           from_role = EXCLUDED.from_role, at = EXCLUDED.at, text = EXCLUDED.text,
           repeat_count = EXCLUDED.repeat_count, report_filed = EXCLUDED.report_filed,
           verdict_accepted = EXCLUDED.verdict_accepted, refreshed_at = now()
    RETURNING 1
), removed AS (
    DELETE FROM memory.seat_memory_items i
     WHERE NOT EXISTS (SELECT 1 FROM fresh f
                        WHERE f.project_id = i.project_id AND f.item_kind = i.item_kind
                          AND f.source_ref = i.source_ref AND f.session_uuid = i.session_uuid)
    RETURNING 1
)
SELECT (SELECT count(*) FROM fresh) AS items,
       (SELECT count(*) FROM written) AS written,
       (SELECT count(*) FROM removed) AS removed
