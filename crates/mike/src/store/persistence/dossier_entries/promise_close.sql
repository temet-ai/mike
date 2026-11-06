WITH open_promises AS (
        SELECT d.id, d.session_id, d.operator_turn_id, d.effective_at, d.data,
               COALESCE(NULLIF(d.data->'backing', 'null'::jsonb), '[]'::jsonb) AS backing_list
          FROM memory.dossier_entries d
         WHERE d.kind = 'promise' AND d.status = 'open' AND d.session_id = ANY($1)
    ), backing AS (
        SELECT p.id AS entry_id, (b->>'work_item_id')::bigint AS work_item_id, b->>'task_id' AS task_id
          FROM open_promises p, jsonb_array_elements(p.backing_list) b
    ), attention_broken AS (
        SELECT DISTINCT ON (p.id) p.id, 'broken'::text AS status, a.first_seen_at AS event_at,
               'agents.work_item_attention:' || a.id AS closed_by
          FROM open_promises p
          JOIN agents.work_item_attention a
            ON a.session_id = p.session_id
           AND a.first_seen_at >= p.effective_at
           AND a.kind IN ('stalled_wait', 'result_lost', 'agent_silent', 'landed_polling')
          JOIN backing bk ON bk.entry_id = p.id AND bk.task_id = split_part(a.subject, '@', 1)
         ORDER BY p.id, a.first_seen_at, a.id
    ), idle_broken AS (
        SELECT DISTINCT ON (p.id) p.id, 'broken'::text AS status, a.first_seen_at AS event_at,
               'agents.work_item_attention:' || a.id AS closed_by
          FROM open_promises p
          JOIN agents.work_item_attention a
            ON a.session_id = p.session_id
           AND a.kind = 'main_idle_with_open_work'
           AND a.first_seen_at > p.effective_at
          JOIN backing bk ON bk.entry_id = p.id
          JOIN agents.work_items w
            ON w.id = bk.work_item_id
           AND (w.finished_at IS NULL OR w.finished_at > a.first_seen_at)
         ORDER BY p.id, a.first_seen_at, a.id
    ), failed_broken AS (
        SELECT DISTINCT ON (p.id) p.id, 'broken'::text AS status, w.finished_at AS event_at,
               'agents.work_items:' || w.id AS closed_by
          FROM open_promises p
          JOIN backing bk ON bk.entry_id = p.id
          JOIN agents.work_items w
            ON w.id = bk.work_item_id
           AND w.state IN ('failed', 'killed')
           AND w.finished_at IS NOT NULL
         ORDER BY p.id, w.finished_at, w.id
    ), unbacked_broken AS (
        SELECT p.id, 'broken'::text AS status, s.last_seen AS event_at,
               'memory.operator_turns:' || p.operator_turn_id AS closed_by
          FROM open_promises p
          JOIN sessions.sessions s ON s.id = p.session_id
         WHERE (p.data->>'registry_coverage')::boolean IS TRUE
           AND jsonb_array_length(p.backing_list) = 0
           AND p.operator_turn_id IS NOT NULL
           AND s.last_seen < now() - make_interval(mins => $2::bigint::int)
           AND NOT EXISTS (
               SELECT 1 FROM requests.requests r
                 JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                                            AND o.code IN ('primary', 'primary_first')
                WHERE r.session_id = p.session_id AND r.created_at > p.effective_at)
           AND NOT EXISTS (
               SELECT 1 FROM tools.uses tu
                WHERE tu.session_id = p.session_id
                  AND tu.tool_name IN ('Monitor', 'ScheduleWakeup', 'CronCreate')
                  AND tu.created_at BETWEEN p.effective_at - interval '24 hours' AND p.effective_at)
    ), kept_ready AS (
        SELECT bk.entry_id, max(w.finished_at) AS last_finished
          FROM backing bk
          JOIN agents.work_items w ON w.id = bk.work_item_id
         GROUP BY bk.entry_id
        HAVING bool_and(w.state = 'completed' AND w.finished_at IS NOT NULL)
    ), kept AS (
        SELECT DISTINCT ON (p.id) p.id, 'kept'::text AS status, r.created_at AS event_at,
               'requests.requests:' || r.id AS closed_by
          FROM open_promises p
          JOIN kept_ready k ON k.entry_id = p.id
          JOIN requests.requests r ON r.session_id = p.session_id AND r.created_at >= k.last_finished
          JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                                     AND o.code IN ('primary', 'primary_first')
         WHERE EXISTS (
               SELECT 1 FROM responses.stream_blocks sb
                 JOIN responses.stream_block_types bt ON bt.id = sb.block_type_id AND bt.code = 'text'
                WHERE sb.request_id = r.id)
         ORDER BY p.id, r.created_at, r.id
    ), superseded AS (
        SELECT DISTINCT ON (p.id) p.id, 'superseded'::text AS status, r.created_at AS event_at,
               'requests.requests:' || r.id AS closed_by
          FROM open_promises p
          JOIN prompt_kinds.classifications c ON c.session_id = p.session_id
          JOIN prompt_kinds.kinds k ON k.id = c.kind_id AND k.code = 'operator'
          JOIN requests.requests r ON r.id = c.request_id AND r.created_at > p.effective_at
         ORDER BY p.id, r.created_at, r.id
    ), events AS (
        SELECT * FROM attention_broken
        UNION ALL SELECT * FROM idle_broken
        UNION ALL SELECT * FROM failed_broken
        UNION ALL SELECT * FROM unbacked_broken
        UNION ALL SELECT * FROM kept
        UNION ALL SELECT * FROM superseded
    )
    SELECT DISTINCT ON (id) id AS "id!", status AS "status!", event_at AS "closed_at!",
           closed_by AS "closed_by!"
      FROM events
     WHERE event_at IS NOT NULL
     ORDER BY id, event_at, closed_by
