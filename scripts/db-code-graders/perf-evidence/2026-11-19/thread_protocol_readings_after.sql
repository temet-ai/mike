WITH lanes AS MATERIALIZED (
        SELECT r.message_count, r.is_tool_loop, r.last_user_message, pr.code AS rule, tor.code AS origin,
               r.created_at >= now() - interval '1 hour' AS recent,
               CASE WHEN r.created_at >= now() - interval '1 hour' AND r.request_envelope IS NOT NULL
                    THEN convert_from(r.request_envelope, 'UTF8')::jsonb -> 'thread' END AS thread
          FROM requests.requests r
          JOIN requests.turn_origins tor ON tor.id = r.turn_origin_id
          LEFT JOIN requests.predecessor_rules pr ON pr.id = r.predecessor_rule_id
         WHERE r.created_at >= now() - interval '24 hours'
           AND tor.code IN ('primary', 'primary_first', 'subagent', 'workflow_subagent')
    )
    SELECT count(*) FILTER (WHERE recent) AS requests,
           count(*) FILTER (WHERE recent AND rule = 'thread_link') AS linked,
           count(*) FILTER (WHERE recent AND thread ->> 'type' = 'continue') AS continues,
           count(*) FILTER (WHERE recent AND thread ->> 'type' = 'continue' AND rule IS DISTINCT FROM 'thread_link') AS unresolved,
           count(*) FILTER (WHERE recent AND thread IS NOT NULL AND coalesce(thread ->> 'type', '') NOT IN ('create', 'continue')) AS unknown_kind,
           count(*) FILTER (WHERE recent AND rule IN ('thread_start', 'thread_restart') AND message_count >= 3) AS resends,
           count(*) FILTER (WHERE recent AND thread IS NULL AND is_tool_loop AND message_count <= 2) AS unseen_continuations,
           count(*) FILTER (WHERE recent AND origin = 'subagent' AND thread IS NULL AND message_count = 1 AND last_user_message LIKE '<task>%') AS label_shaped,
           count(*) FILTER (WHERE NOT recent) AS day_requests,
           count(*) FILTER (WHERE NOT recent AND rule = 'thread_link') AS day_linked
      FROM lanes;
