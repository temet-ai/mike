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
           SELECT r.thread_id AS "thread_id!", r.id AS "id!",
                  r.operator_turn_id AS "operator_turn_id?", r.text AS "text?",
                  r.effective_at AS "effective_at!"
             FROM (
                   SELECT b.thread_id, d.id, d.operator_turn_id, d.effective_at,
                          CASE WHEN ((d.data -> 'span') ->> 0) ~ '^[0-9]{1,9}$'
                                AND ((d.data -> 'span') ->> 1) ~ '^[0-9]{1,9}$'
                               THEN substring(ot.final_assistant_text
                                     FROM ((d.data -> 'span') ->> 0)::int + 1
                                     FOR GREATEST(((d.data -> 'span') ->> 1)::int
                                                  - ((d.data -> 'span') ->> 0)::int, 0))
                          END AS text,
                          row_number() OVER (PARTITION BY b.thread_id
                                             ORDER BY d.effective_at DESC, d.id DESC) AS rn
                     FROM bounds b
                     JOIN work.thread_sessions ts ON ts.thread_id = b.thread_id
                     JOIN sessions.sessions se ON se.session_uuid = ts.session_uuid
                     JOIN memory.dossier_entries d
                       ON d.session_id = se.id AND d.kind = 'promise' AND d.status = 'open'
                      AND d.effective_at > b.since
                     LEFT JOIN memory.operator_turns ot ON ot.id = d.operator_turn_id
                  ) r
            WHERE r.rn <= 2
            ORDER BY r.thread_id, r.effective_at DESC
