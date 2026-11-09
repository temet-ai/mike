SELECT a.id AS lesson_a_id, a.text AS lesson_a_text, a.created_at AS lesson_a_created_at,
                          b.id AS lesson_b_id, b.text AS lesson_b_text, b.created_at AS lesson_b_created_at,
                          p.distance AS distance
                     FROM memory.lesson_pair_candidates p
                     JOIN memory.lessons a ON a.id = p.lesson_a_id
                     JOIN memory.lessons b ON b.id = p.lesson_b_id AND b.project_id = a.project_id
                     JOIN memory.lesson_stages sa ON sa.id = a.stage_id AND sa.code IN ('daily', 'long_term')
                     JOIN memory.lesson_stages sb ON sb.id = b.stage_id AND sb.code IN ('daily', 'long_term')
                    WHERE p.judged_at IS NULL
                      AND a.project_id = ANY(ARRAY[1,2,3,4,5,6,7,8,9,10]::bigint[])
                      AND NOT EXISTS (
                          SELECT 1 FROM memory.lesson_relations r
                           WHERE (r.from_lesson_id = a.id AND r.to_lesson_id = b.id)
                              OR (r.from_lesson_id = b.id AND r.to_lesson_id = a.id)
                      )
                    ORDER BY p.score DESC, p.lesson_a_id, p.lesson_b_id
                    LIMIT 100;
