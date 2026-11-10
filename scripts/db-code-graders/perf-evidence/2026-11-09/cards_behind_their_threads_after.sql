SELECT t.id AS thread_id,
                          GREATEST(t.updated_at, s.updated_at, l.max_at, ts.max_at, cc.max_at) AS observed_at
                     FROM work.threads t
                     LEFT JOIN work.thread_state s ON s.thread_id = t.id
                     LEFT JOIN work.thread_cards c ON c.thread_id = t.id
                     LEFT JOIN LATERAL (
                          SELECT max(lg.created_at) AS max_at
                            FROM work.thread_log lg
                           WHERE lg.thread_id = t.id
                     ) l ON true
                     LEFT JOIN LATERAL (
                          SELECT max(x.first_seen_at) AS max_at
                            FROM work.thread_sessions x
                           WHERE x.thread_id = t.id
                     ) ts ON true
                     LEFT JOIN LATERAL (
                          SELECT max(k.built_at) AS max_at
                            FROM work.thread_card_candidates k
                           WHERE k.thread_id = t.id
                     ) cc ON true
                    WHERE (t.status IS NULL OR t.status NOT IN ('done', 'complete'))
                      AND (c.thread_id IS NULL
                           OR GREATEST(t.updated_at, s.updated_at, l.max_at, ts.max_at, cc.max_at)
                              > c.source_max_at)
                    ORDER BY t.id;
