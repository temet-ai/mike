WITH proj AS (
                       SELECT id FROM projects.projects WHERE project_uuid = '1e4a4c17-3a4c-4244-b532-4c89b304e472'::uuid),
                   folders AS (
                       SELECT d.folder FROM memory.skill_drafts d JOIN proj ON proj.id = d.project_id
                       UNION SELECT i.folder FROM memory.skill_installs i JOIN proj ON proj.id = i.project_id
                       UNION SELECT r.folder FROM memory.skill_retirements r JOIN proj ON proj.id = r.project_id),
                   latest AS (
                       SELECT DISTINCT ON (d.folder) d.folder, d.id, d.status, d.review, d.revision,
                              d.lesson_watermark, cardinality(d.lesson_ids) AS lesson_count,
                              d.description, d.created_at, d.evaluated_at, d.edited_at,
                              (d.eval -> 'trigger' ->> 'score')::float8 AS trigger_score,
                              (d.eval -> 'usefulness' -> 'with_skill' ->> 'rate')::float8 AS with_skill_rate,
                              (d.eval -> 'usefulness' -> 'without_skill' ->> 'rate')::float8 AS without_skill_rate,
                              d.eval -> 'verdict' -> 'reasons' AS verdict_reasons
                         FROM memory.skill_drafts d JOIN proj ON proj.id = d.project_id
                        ORDER BY d.folder, d.lesson_watermark DESC, d.revision DESC),
                   approved AS (
                       SELECT DISTINCT ON (d.folder) d.folder, d.id
                         FROM memory.skill_drafts d JOIN proj ON proj.id = d.project_id
                        WHERE d.review = 'approved'
                        ORDER BY d.folder, d.lesson_watermark DESC, d.revision DESC),
                   installs AS (
                       SELECT i.folder, min(i.first_written) AS first_written,
                              bool_or(i.retired_at IS NULL) AS live,
                              jsonb_agg(jsonb_build_object(
                                  'machine_name', i.machine_name, 'path', i.skill_path,
                                  'draft_id', i.draft_id, 'first_written', i.first_written,
                                  'retired_at', i.retired_at, 'retired_reason', i.retired_reason,
                                  'reported_at', i.reported_at)
                                  ORDER BY i.machine_name, i.skill_path) AS installs
                         FROM memory.skill_installs i JOIN proj ON proj.id = i.project_id
                        GROUP BY i.folder),
                   loaded AS (
                       SELECT s.folder, t.session_id, t.created_at,
                              (toc.code = 'corrected' OR (toc.code = 'abandoned'
                                AND t.outcome_source IS NOT DISTINCT FROM 'frustration_close')) AS corrected
                         FROM memory.operator_turns t
                         JOIN proj ON proj.id = t.project_id
                         JOIN memory.turn_outcomes toc ON toc.id = t.outcome_id
                        CROSS JOIN unnest(t.skills_used) AS s(folder)
                        WHERE t.skills_used IS NOT NULL),
                   loads AS (
                       SELECT ld.folder, count(*) AS loads, count(DISTINCT ld.session_id) AS sessions,
                              count(*) FILTER (WHERE ld.corrected) AS corrected,
                              max(ld.created_at) AS last_load_at
                         FROM loaded ld
                         JOIN installs i ON i.folder = ld.folder AND ld.created_at >= i.first_written
                        GROUP BY ld.folder)
                   SELECT f.folder AS "folder!", l.id AS "draft_id?", l.status AS "status?",
                          l.review AS "review?", l.revision AS "revision?",
                          l.lesson_watermark AS "lesson_watermark?",
                          l.lesson_count AS "lesson_count?", l.description AS "description?",
                          l.created_at AS "drafted_at?", l.evaluated_at AS "evaluated_at?",
                          l.edited_at AS "edited_at?", l.trigger_score AS "trigger_score?",
                          l.with_skill_rate AS "with_skill_rate?",
                          l.without_skill_rate AS "without_skill_rate?",
                          l.verdict_reasons AS "verdict_reasons?",
                          a.id AS "approved_draft_id?", i.first_written AS "first_written?",
                          coalesce(i.live, false) AS "live!",
                          coalesce(i.installs, '[]'::jsonb) AS "installs!",
                          r.retired_at AS "retired_by_user_at?",
                          coalesce(lo.loads, 0) AS "loads!",
                          coalesce(lo.sessions, 0) AS "load_sessions!",
                          coalesce(lo.corrected, 0) AS "corrected_after_load!",
                          lo.last_load_at AS "last_load_at?"
                     FROM folders f
                    CROSS JOIN proj
                     LEFT JOIN latest l ON l.folder = f.folder
                     LEFT JOIN approved a ON a.folder = f.folder
                     LEFT JOIN installs i ON i.folder = f.folder
                     LEFT JOIN loads lo ON lo.folder = f.folder
                     LEFT JOIN memory.skill_retirements r
                       ON r.project_id = proj.id AND r.folder = f.folder
                    ORDER BY f.folder
