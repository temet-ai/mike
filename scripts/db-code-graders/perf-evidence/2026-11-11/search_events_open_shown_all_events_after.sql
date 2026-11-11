WITH open_events AS (
    SELECT e.id, e.session_id, e.request_id, e.created_at, e.returned
      FROM memory.search_events e
     WHERE e.used_final_at IS NULL AND e.session_id IS NOT NULL AND e.request_id IS NOT NULL
     ORDER BY e.used_checked_at NULLS FIRST, e.created_at
     LIMIT 250::bigint
), shown AS (
    SELECT e.id AS event_id, r.value ->> 'reference' AS reference,
           left(r.value ->> 'shown', 1500) AS shown_text, r.ordinality AS position
      FROM open_events e
      CROSS JOIN LATERAL jsonb_array_elements(e.returned) WITH ORDINALITY AS r(value, ordinality)
     WHERE r.ordinality <= 10 AND (r.value ->> 'reference') IS NOT NULL
), shown_events AS (
    SELECT DISTINCT event_id FROM shown
), windows AS (
    SELECT s.event_id, w.final_assistant_text, w.user_prompt, w.start_request_id
      FROM shown_events s
      JOIN open_events e ON e.id = s.event_id
      CROSS JOIN LATERAL (
          SELECT t.final_assistant_text, t.user_prompt, t.start_request_id
            FROM memory.operator_turns t
           WHERE t.session_id = e.session_id AND t.end_request_id >= e.request_id
           ORDER BY t.start_request_id
           LIMIT 6::bigint
      ) w
     WHERE w.final_assistant_text IS NOT NULL
)
SELECT s.event_id AS "event_id!", s.position AS "position!", s.reference AS "reference?",
       s.shown_text AS "shown_text?", NULL::text AS "answer?", NULL::text AS "prompt?"
  FROM shown s
UNION ALL
SELECT w.event_id, 0::bigint, NULL::text, NULL::text, w.final_assistant_text, w.user_prompt
  FROM windows w
 ORDER BY 1, 2
