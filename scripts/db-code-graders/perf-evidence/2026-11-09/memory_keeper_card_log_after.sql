SELECT r.thread_id AS "thread_id!", r.id AS "id!", r.kind AS "kind!",
                  r.entry_date AS "entry_date!", r.session_uuid AS "session_uuid?",
                  r.summary AS "summary!"
             FROM (
                   SELECT l.thread_id, l.id, l.kind, l.entry_date, l.session_uuid, l.summary, l.ord,
                          row_number() OVER (PARTITION BY l.thread_id, l.kind ORDER BY l.ord DESC) AS rn,
                          max(l.ord) FILTER (WHERE l.kind = 'handoff')
                              OVER (PARTITION BY l.thread_id) AS handoff_ord
                     FROM work.thread_log l
                    WHERE l.thread_id = ANY(ARRAY(SELECT ot.id FROM work.threads ot WHERE ot.status IS NULL OR ot.status NOT IN ('done', 'complete'))) AND l.kind IN ('handoff', 'decision')
                  ) r
            WHERE (r.kind = 'handoff' AND r.rn = 1)
               OR (r.kind = 'decision' AND r.rn <= 3
                   AND (r.handoff_ord IS NULL OR r.ord > r.handoff_ord))
            ORDER BY r.thread_id, r.ord DESC
