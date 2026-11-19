WITH cursor AS (
        SELECT 700000::bigint AS last_request_id
    ),
    batch AS (
        SELECT r.id, r.predecessor_request_id, r.message_count, tor.code AS origin,
               CASE WHEN r.request_envelope IS NOT NULL
                    THEN convert_from(r.request_envelope, 'UTF8')::jsonb -> 'thread' END AS thread
          FROM requests.requests r
          JOIN requests.turn_origins tor ON tor.id = r.turn_origin_id
         WHERE r.id > (SELECT last_request_id FROM cursor)
           AND r.predecessor_rule_id IS NULL
         ORDER BY r.id
         LIMIT 500
    ),
    classified AS (
        SELECT b.id,
               CASE WHEN l.request_id IS NOT NULL THEN 'thread_link'
                    WHEN b.thread ->> 'type' = 'create' AND b.message_count >= 3
                         AND b.predecessor_request_id IS NOT NULL THEN 'thread_restart'
                    WHEN b.thread ->> 'type' = 'create' THEN 'thread_start'
                    WHEN b.predecessor_request_id IS NOT NULL THEN 'lane_fallback'
                    ELSE 'none' END AS rule,
               CASE WHEN l.request_id IS NOT NULL THEN l.request_id
                    WHEN b.thread ->> 'type' = 'create' AND b.message_count < 3 THEN NULL
                    ELSE b.predecessor_request_id END AS predecessor,
               (l.request_id IS NOT NULL
                OR b.thread ->> 'type' = 'create'
                OR b.origin = 'agent_label_sidecar') AS drop_removed
          FROM batch b
          LEFT JOIN LATERAL (
              SELECT rr.request_id
                FROM responses.responses rr
               WHERE b.thread ->> 'type' = 'continue'
                 AND rr.upstream_message_id = b.thread ->> 'previous_message_id'
               ORDER BY rr.request_id DESC
               LIMIT 1
          ) l ON true
    ),
    deleted AS (
        DELETE FROM requests.deltas d
         USING classified c
         WHERE d.request_id = c.id
           AND c.drop_removed
           AND d.change_kind_id = (SELECT id FROM requests.delta_change_kinds WHERE code = 'removed')
        RETURNING d.id
    ),
    repointed AS (
        UPDATE requests.deltas d
           SET predecessor_request_id = c.predecessor
          FROM classified c
         WHERE d.request_id = c.id
           AND c.drop_removed
           AND d.change_kind_id = (SELECT id FROM requests.delta_change_kinds WHERE code = 'added')
           AND d.predecessor_request_id IS DISTINCT FROM c.predecessor
        RETURNING d.id
    ),
    stamped AS (
        UPDATE requests.requests r
           SET predecessor_request_id = c.predecessor,
               predecessor_rule_id = pr.id
          FROM classified c
          JOIN requests.predecessor_rules pr ON pr.code = c.rule
         WHERE r.id = c.id
        RETURNING r.id
    )
    SELECT (SELECT max(id) FROM stamped) AS through,
           (SELECT count(*) FROM stamped) AS corrected,
           (SELECT count(*) FROM deleted) AS deleted;
