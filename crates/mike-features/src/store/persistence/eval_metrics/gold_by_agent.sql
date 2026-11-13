SELECT s.at AS "at!", q.subject AS "subject!", q.detail AS "detail!"
  FROM unnest($1::timestamptz[]) AS s(at)
 CROSS JOIN LATERAL (
    WITH cmd AS (
        SELECT u.id, u.request_id, u.created_at, u.input ->> 'command' AS command, r.session_id
          FROM tools.uses u
          JOIN requests.requests r ON r.id = u.request_id
         WHERE u.tool_name = 'Bash'
           AND u.created_at >= s.at - interval '24 hours' AND u.created_at < s.at
           AND (u.input ->> 'command') ~ $2
    ),
    matched AS (
        SELECT cmd.id, jsonb_agg(jsonb_build_object('label_id', l.id, 'provenance', l.provenance)
                                 ORDER BY l.id) AS labels
          FROM cmd
          JOIN eval.labels l ON l.tier = 'gold' AND l.source = 'spa'
                            AND l.created_at >= cmd.created_at
                            AND l.created_at <= cmd.created_at + interval '120 seconds'
         GROUP BY cmd.id
    )
    SELECT cmd.id AS cmd_id, 'tool_use:' || cmd.id::text AS subject,
           jsonb_build_object('command', cmd.command, 'session_id', cmd.session_id,
                              'request_id', cmd.request_id, 'labels', matched.labels) AS detail
      FROM cmd JOIN matched ON matched.id = cmd.id
     ORDER BY cmd.id
 ) q
 ORDER BY s.at, q.cmd_id
