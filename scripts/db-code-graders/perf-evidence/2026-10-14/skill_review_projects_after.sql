WITH touched AS (
                       SELECT project_id FROM memory.skill_drafts
                       UNION SELECT project_id FROM memory.skill_installs
                       UNION SELECT project_id FROM memory.skill_retirements),
                   latest AS (
                       SELECT DISTINCT ON (d.project_id, d.folder)
                              d.project_id, d.folder, d.id, d.status, d.review
                         FROM memory.skill_drafts d
                        ORDER BY d.project_id, d.folder, d.lesson_watermark DESC, d.revision DESC),
                   approved AS (
                       SELECT DISTINCT ON (d.project_id, d.folder) d.project_id, d.folder, d.id
                         FROM memory.skill_drafts d
                        WHERE d.review = 'approved'
                        ORDER BY d.project_id, d.folder, d.lesson_watermark DESC, d.revision DESC),
                   pending AS (
                       SELECT l.project_id, count(*) AS folders,
                              count(*) FILTER (WHERE l.status = 'accepted' AND l.review IS NULL
                                                AND a.id IS DISTINCT FROM l.id) AS awaiting
                         FROM latest l
                         LEFT JOIN approved a ON a.project_id = l.project_id AND a.folder = l.folder
                        WHERE NOT EXISTS (SELECT 1 FROM memory.skill_retirements r
                                           WHERE r.project_id = l.project_id AND r.folder = l.folder)
                        GROUP BY l.project_id),
                   shipped AS (
                       SELECT i.project_id, count(DISTINCT i.folder) AS shipped
                         FROM memory.skill_installs i
                        WHERE i.retired_at IS NULL
                        GROUP BY i.project_id)
                   SELECT p.project_uuid AS "project_uuid!", p.project_path AS "project_path!",
                          coalesce(p.custom_name, p.display_name) AS "display_name!",
                          coalesce(sp.mode, 'ask') AS "mode!",
                          sp.project_id IS NOT NULL AS "mode_set!",
                          coalesce(pe.folders, 0) AS "folders!",
                          coalesce(s.shipped, 0) AS "shipped!",
                          CASE WHEN coalesce(sp.mode, 'ask') = 'ask'
                               THEN coalesce(pe.awaiting, 0) ELSE 0 END AS "waiting!"
                     FROM touched t
                     JOIN projects.projects p ON p.id = t.project_id
                     LEFT JOIN memory.skill_policies sp ON sp.project_id = p.id
                     LEFT JOIN pending pe ON pe.project_id = p.id
                     LEFT JOIN shipped s ON s.project_id = p.id
                    ORDER BY p.project_path, p.id
