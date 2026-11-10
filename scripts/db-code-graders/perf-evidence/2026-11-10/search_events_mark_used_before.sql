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
    SELECT e.id AS event_id, r.value ->> 'reference' AS reference,
           regexp_replace(r.value ->> 'reference', '([^0-9A-Za-z:_-])', '\\\1', 'g') AS pattern
      FROM open_events e
      CROSS JOIN LATERAL jsonb_array_elements(e.returned) AS r(value)
), reads AS (
    SELECT e.id AS event_id,
           CASE WHEN u.tool_name ~ '(^|__)read_project_knowledge$' THEN u.input ->> 'reference'
                ELSE 'turn:' || (u.input ->> 'id') END AS reference
      FROM open_events e
      JOIN bounds b ON b.event_id = e.id
      JOIN tools.uses u ON u.session_id = e.session_id
                       AND u.created_at >= e.created_at
                       AND u.created_at <= b.last_at + interval '1 minute'
                       AND u.request_id > e.request_id
                       AND u.request_id <= b.last_request_id
     WHERE u.tool_name ~ '(^|__)(read_project_knowledge|get_operator_turn)$'
), replies AS (
    SELECT r.event_id, r.reference
      FROM returned r
      JOIN windows w ON w.event_id = r.event_id
     WHERE w.final_assistant_text ~ ('(^|[^0-9A-Za-z_-])' || r.pattern || '($|[^0-9A-Za-z_-])')
), used AS (
    SELECT event_id, reference FROM reads
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
