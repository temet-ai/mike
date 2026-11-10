WITH open_events AS (
    SELECT e.id, e.session_id, e.request_id, e.created_at, e.returned
      FROM memory.search_events e
     WHERE e.used_final_at IS NULL AND e.session_id IS NOT NULL AND e.request_id IS NOT NULL
     ORDER BY e.used_checked_at NULLS FIRST, e.created_at
     LIMIT 250::bigint
), windows AS (
    SELECT e.id AS event_id, w.end_request_id, w.closed_at, w.final_assistant_text
      FROM open_events e
      CROSS JOIN LATERAL (
          SELECT t.end_request_id, t.closed_at, t.final_assistant_text
            FROM memory.operator_turns t
           WHERE t.session_id = e.session_id AND t.end_request_id >= e.request_id
           ORDER BY t.start_request_id
           LIMIT 6::bigint
      ) w
), bounds AS (
    SELECT event_id, count(*) AS turn_count, max(end_request_id) AS last_request_id,
           max(COALESCE(closed_at, now())) AS last_at,
           bool_and(closed_at IS NOT NULL) AS all_closed
      FROM windows
     GROUP BY event_id
), returned AS (
    SELECT p.event_id, p.reference, p.kind, p.id_part, lower(p.id_part) AS id_lower,
           regexp_replace(p.reference, '([^0-9A-Za-z:_-])', '\\\1', 'g') AS pattern,
           regexp_replace(p.id_part, '([^0-9A-Za-z:_-])', '\\\1', 'g') AS id_pattern,
           CASE WHEN p.id_part ~ '^[0-9]{1,18}$' THEN p.id_part::bigint END AS numeric_id
      FROM (SELECT e.id AS event_id, r.value ->> 'reference' AS reference,
                   split_part(r.value ->> 'reference', ':', 1) AS kind,
                   substr(r.value ->> 'reference', strpos(r.value ->> 'reference', ':') + 1) AS id_part
              FROM open_events e
              CROSS JOIN LATERAL jsonb_array_elements(e.returned) AS r(value)) p
), calls AS (
    SELECT e.id AS event_id, e.session_id AS caller_session_id, k.kind,
           lower(u.input ->> 'session_id') AS session_arg, lower(u.input ->> 'id') AS id_arg,
           u.input ->> 'reference' AS reference_arg,
           lower(u.input ->> 'reference') AS reference_lower,
           coalesce(u.input ->> 'reference' ~ '^[0-9A-Fa-f]{8,}$', false) AS reference_is_hex,
           u.input ->> 'slug' AS slug_arg, u.input ->> 'thread' AS thread_arg
      FROM open_events e
      JOIN bounds b ON b.event_id = e.id
      JOIN tools.uses u ON u.session_id = e.session_id
                       AND u.created_at >= e.created_at
                       AND u.created_at <= b.last_at + interval '1 minute'
                       AND u.request_id > e.request_id
                       AND u.request_id <= b.last_request_id
     CROSS JOIN LATERAL (SELECT CASE WHEN u.tool_name !~ '(^|__)(read_project_knowledge|get_operator_turn|work_thread_get)$' THEN 'other'
                                     WHEN u.tool_name LIKE '%read_project_knowledge' THEN 'read'
                                     WHEN u.tool_name LIKE '%get_operator_turn' THEN 'turn'
                                     ELSE 'thread' END AS kind) k
     WHERE k.kind <> 'other'
        OR (starts_with(u.tool_name, 'mcp__')
            AND jsonb_exists_any(u.input, ARRAY['session_id', 'id']))
), reads AS (
    SELECT c.event_id,
           CASE c.kind WHEN 'read' THEN c.reference_arg ELSE 'turn:' || c.id_arg END AS reference
      FROM calls c
     WHERE c.kind IN ('read', 'turn')
), bare_reads AS (
    SELECT c.event_id, r.reference
      FROM calls c
      JOIN returned r ON r.event_id = c.event_id
     WHERE c.kind = 'read'
       AND (c.reference_arg = r.id_part
            OR (c.reference_is_hex AND starts_with(r.id_lower, c.reference_lower)))
), returned_turns AS (
    SELECT r.event_id, r.reference, t.session_id AS source_session_id
      FROM returned r
      JOIN memory.operator_turns t ON t.id = r.numeric_id
     WHERE r.kind = 'turn'
    UNION ALL
    SELECT r.event_id, r.reference, t.session_id
      FROM returned r
      JOIN memory.turn_verdicts tv ON tv.id = r.numeric_id
      JOIN memory.operator_turns t ON t.id = tv.operator_turn_id
     WHERE r.kind IN ('correction', 'unmet_ask')
), opened_sessions AS (
    SELECT rt.event_id, rt.reference
      FROM returned_turns rt
      JOIN sessions.sessions s ON s.id = rt.source_session_id
     WHERE EXISTS (
           SELECT 1 FROM calls c
            WHERE c.event_id = rt.event_id AND c.caller_session_id <> rt.source_session_id
              AND (c.session_arg = s.session_uuid::text OR c.id_arg = s.session_uuid::text))
), thread_reads AS (
    SELECT r.event_id, r.reference
      FROM returned r
     WHERE r.kind = 'work_thread'
       AND EXISTS (
           SELECT 1 FROM calls c
            WHERE c.event_id = r.event_id AND c.kind = 'thread'
              AND (c.slug_arg = r.id_part OR c.thread_arg = r.id_part))
), replies AS (
    SELECT r.event_id, r.reference
      FROM returned r
      JOIN windows w ON w.event_id = r.event_id
     WHERE w.final_assistant_text ~ ('(^|[^0-9A-Za-z_-])' || r.pattern || '($|[^0-9A-Za-z_-])')
    UNION
    SELECT r.event_id, r.reference
      FROM returned r
      JOIN windows w ON w.event_id = r.event_id
     WHERE r.kind = 'work_thread'
       AND w.final_assistant_text ~ ('(^|[^0-9A-Za-z_-])' || r.id_pattern || '($|[^0-9A-Za-z_-])')
), used AS (
    SELECT event_id, reference FROM reads
    UNION
    SELECT event_id, reference FROM bare_reads
    UNION
    SELECT event_id, reference FROM opened_sessions
    UNION
    SELECT event_id, reference FROM thread_reads
    UNION
    SELECT event_id, reference FROM replies
), used_in_returned AS (
    SELECT u.event_id, array_agg(DISTINCT u.reference ORDER BY u.reference) AS refs
      FROM used u
      JOIN returned r ON r.event_id = u.event_id AND r.reference = u.reference
     GROUP BY u.event_id
), outcome AS (
    SELECT e.id,
           COALESCE(ur.refs, '{}'::text[]) AS refs,
           COALESCE(b.turn_count = 6::bigint AND b.all_closed, false)
               OR e.created_at < now() - make_interval(secs => 86400::double precision) AS final
      FROM open_events e
      LEFT JOIN bounds b ON b.event_id = e.id
      LEFT JOIN used_in_returned ur ON ur.event_id = e.id
)
UPDATE memory.search_events s
   SET used_references = o.refs,
       used_final_at = CASE WHEN o.final THEN now() END,
       used_checked_at = now()
  FROM outcome o
 WHERE s.id = o.id
RETURNING s.id AS "id!", cardinality(s.used_references)::bigint AS "used!",
          (s.used_final_at IS NOT NULL) AS "finalised!"
