SELECT l.id AS lesson_id, pl.code AS placement, k.code AS kind, st.code AS stage,
                          l.text, l.topic, l.placement_covered_by AS covered_by,
                          l.placement_reason AS reason, a.name AS agent_name,
                          t.user_prompt AS user_prompt, l.created_at, l.placed_at,
                          r.decision AS decision, r.decided_at AS decided_at
                     FROM memory.lessons l
                     JOIN projects.projects p ON p.id = l.project_id
                     JOIN memory.lesson_placements pl ON pl.id = l.placement_id
                     JOIN memory.lesson_kinds k ON k.id = l.kind_id
                     JOIN memory.lesson_stages st ON st.id = l.stage_id
                     LEFT JOIN agents.agents a ON a.id = l.agent_id
                     LEFT JOIN memory.operator_turns t ON t.id = l.source_turn_id
                     LEFT JOIN memory.lesson_placement_reviews r ON r.lesson_id = l.id
                    WHERE p.project_uuid = '83e1cf0a-c665-42b4-b16f-a54f39fccac9'::uuid AND pl.code IN ('rule', 'agent')
                      AND l.archived_at IS NULL
                    ORDER BY l.id;
