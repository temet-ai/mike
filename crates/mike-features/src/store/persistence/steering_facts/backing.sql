WITH k AS (
    SELECT turn_id, session_id, as_of
      FROM unnest($1::bigint[], $2::bigint[], $3::timestamptz[]) AS k(turn_id, session_id, as_of)
),
wi AS (
    SELECT k.turn_id, 'work_item'::text AS source, w.id, w.kind, w.state, w.launched_at,
           NULL::timestamptz AS due_at, w.owner_agent_id, NULL::text AS peer_session_uuid,
           NULL::text AS message_uuid, NULL::bigint AS directive_id
      FROM k
      JOIN agents.work_items w ON w.session_id = k.session_id
     WHERE w.launched_at <= k.as_of
       AND (w.finished_at IS NULL OR w.finished_at > k.as_of)
       AND NOT EXISTS (SELECT 1 FROM agents.work_items o
                        WHERE o.session_id = w.session_id AND o.kind = 'agent'
                          AND o.task_id = w.owner_agent_id
                          AND o.finished_at IS NOT NULL AND o.finished_at <= k.as_of)
),
peer(turn_id, source, id, kind, state, launched_at, due_at, owner_agent_id, peer_session_uuid, message_uuid, directive_id) AS (
    SELECT k.turn_id, 'peer_reply'::text, m.id, NULL::text, NULL::text, m.queued_at,
           NULL::timestamptz, NULL::text, s.session_uuid::text, m.message_uuid::text, NULL::bigint
      FROM k
      JOIN mailbox.messages m ON m.from_session_id = k.session_id
      JOIN sessions.sessions s ON s.id = m.to_session_id
      JOIN workspace.session_registrations r ON r.session_id = m.to_session_id
     WHERE m.queued_at >= k.as_of - interval '2 hours' AND m.queued_at <= k.as_of
       AND m.dead_lettered_at IS NULL
       AND r.registered_at <= k.as_of AND (r.released_at IS NULL OR r.released_at > k.as_of)
       AND (r.lease_expires_at > k.as_of
            OR EXISTS (SELECT 1 FROM requests.requests q
                        WHERE q.session_id = m.to_session_id
                          AND q.created_at > k.as_of - interval '30 minutes'
                          AND q.created_at <= k.as_of))
       AND NOT EXISTS (SELECT 1 FROM mailbox.messages rep
                        WHERE rep.from_session_id = m.to_session_id AND rep.to_session_id = k.session_id
                          AND rep.queued_at > m.queued_at AND rep.queued_at <= k.as_of)
),
dir(turn_id, source, id, kind, state, launched_at, due_at, owner_agent_id, peer_session_uuid, message_uuid, directive_id) AS (
    SELECT k.turn_id, 'operator_requested'::text, NULL::bigint, NULL::text, e.status, e.effective_at,
           NULL::timestamptz, NULL::text, NULL::text, NULL::text, e.id
      FROM k
      JOIN memory.dossier_entries e ON e.session_id = k.session_id
     WHERE e.kind = 'directive' AND e.data ->> 'class' = 'wait' AND e.data ->> 'polarity' = 'allow'
       AND e.recorded_at <= k.as_of
       AND (e.close_recorded_at IS NULL OR e.close_recorded_at > k.as_of)
),
unreg(turn_id, source, id, kind, state, launched_at, due_at, owner_agent_id, peer_session_uuid, message_uuid, directive_id) AS (
    SELECT k.turn_id, 'unregistered_launch'::text, u.id, u.tool_name, NULL::text, u.created_at,
           NULL::timestamptz, NULL::text, NULL::text, NULL::text, NULL::bigint
      FROM k
      JOIN requests.requests q ON q.session_id = k.session_id
                              AND q.created_at > k.as_of - interval '24 hours'
                              AND q.created_at <= k.as_of
      JOIN tools.uses u ON u.request_id = q.id
     WHERE $4 AND u.tool_name IN ('Monitor', 'ScheduleWakeup', 'CronCreate')
)
SELECT turn_id AS "turn_id!", source AS "source!", id AS "id?", kind AS "kind?", state AS "state?", launched_at AS "launched_at?", due_at AS "due_at?", owner_agent_id AS "owner_agent_id?", peer_session_uuid AS "peer_session_uuid?", message_uuid AS "message_uuid?", directive_id AS "directive_id?" FROM wi
UNION ALL SELECT turn_id, source, id, kind, state, launched_at, due_at, owner_agent_id, peer_session_uuid, message_uuid, directive_id FROM peer
UNION ALL SELECT turn_id, source, id, kind, state, launched_at, due_at, owner_agent_id, peer_session_uuid, message_uuid, directive_id FROM dir
UNION ALL SELECT turn_id, source, id, kind, state, launched_at, due_at, owner_agent_id, peer_session_uuid, message_uuid, directive_id FROM unreg
