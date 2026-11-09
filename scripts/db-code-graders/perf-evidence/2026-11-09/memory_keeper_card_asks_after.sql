WITH handoffs AS (
               SELECT DISTINCT ON (l.thread_id) l.thread_id, l.entry_date
                 FROM work.thread_log l
                WHERE l.thread_id = ANY(ARRAY(SELECT ot.id FROM work.threads ot WHERE ot.status IS NULL OR ot.status NOT IN ('done', 'complete'))) AND l.kind = 'handoff'
                ORDER BY l.thread_id, l.ord DESC
           ),
           bounds AS (
               SELECT t.id AS thread_id,
                      GREATEST(now() - make_interval(days => 14),
                               h.entry_date::timestamp AT TIME ZONE 'UTC') AS since
                 FROM work.threads t
                 LEFT JOIN handoffs h ON h.thread_id = t.id
                WHERE t.id = ANY(ARRAY(SELECT ot.id FROM work.threads ot WHERE ot.status IS NULL OR ot.status NOT IN ('done', 'complete')))
           )
           SELECT r.thread_id AS "thread_id!", r.operator_turn_id AS "operator_turn_id!",
                  r.unmet_ask AS "unmet_ask!", r.created_at AS "created_at!"
             FROM (
                   SELECT latest.thread_id, latest.operator_turn_id, latest.unmet_ask, latest.created_at,
                          row_number() OVER (PARTITION BY latest.thread_id
                                             ORDER BY latest.created_at DESC,
                                                      latest.operator_turn_id DESC) AS rn
                     FROM (
                           SELECT DISTINCT ON (b.thread_id, v.operator_turn_id)
                                  b.thread_id, v.operator_turn_id, v.unmet_ask, v.created_at
                             FROM bounds b
                             JOIN work.thread_sessions ts ON ts.thread_id = b.thread_id
                             JOIN sessions.sessions se ON se.session_uuid = ts.session_uuid
                             JOIN memory.turn_verdicts v
                               ON v.session_id = se.id AND v.mode = 'live' AND v.created_at > b.since
                            ORDER BY b.thread_id, v.operator_turn_id, v.created_at DESC, v.id DESC
                          ) latest
                    WHERE btrim(latest.unmet_ask) <> ''
                  ) r
            WHERE r.rn <= 2
            ORDER BY r.thread_id, r.created_at DESC
