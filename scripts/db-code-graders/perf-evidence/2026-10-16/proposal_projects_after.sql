SELECT p.project_uuid AS project_uuid, p.project_path AS project_path,
                          coalesce(p.custom_name, p.display_name) AS display_name,
                          count(*) AS proposals,
                          count(*) FILTER (WHERE r.lesson_id IS NULL) AS undecided
                     FROM memory.lessons l
                     JOIN memory.lesson_placements pl ON pl.id = l.placement_id
                     JOIN projects.projects p ON p.id = l.project_id
                     LEFT JOIN memory.lesson_placement_reviews r ON r.lesson_id = l.id
                    WHERE pl.code IN ('rule', 'agent') AND l.archived_at IS NULL
                    GROUP BY p.id
                    ORDER BY p.project_path, p.id;
