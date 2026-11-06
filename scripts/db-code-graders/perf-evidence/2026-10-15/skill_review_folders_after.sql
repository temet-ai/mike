WITH proj AS (
                       SELECT id FROM projects.projects WHERE project_uuid = 'ec5bcb17-7792-47c2-9f7b-c07b1f417f92'::uuid),
                   folders AS (
                       SELECT d.folder FROM memory.skill_drafts d JOIN proj ON proj.id = d.project_id
                       UNION SELECT i.folder FROM memory.skill_installs i JOIN proj ON proj.id = i.project_id
                       UNION SELECT r.folder FROM memory.skill_retirements r JOIN proj ON proj.id = r.project_id
                       UNION SELECT o.folder FROM memory.skill_live_outcomes o JOIN proj ON proj.id = o.project_id),
                   latest AS (
                       SELECT DISTINCT ON (d.folder) d.folder, d.id, d.status, d.review, d.revision,
                              d.lesson_watermark, cardinality(d.lesson_ids) AS lesson_count,
                              d.description, d.created_at, d.evaluated_at, d.edited_at,
                              (d.eval -> 'trigger' ->> 'score')::float8 AS trigger_score,
                              (d.eval -> 'recheck' ->> 'score')::float8 AS patched_trigger_score,
                              CASE WHEN d.eval ? 'review' THEN jsonb_build_object(
                                  'decision', d.eval #>> '{review,decision}',
                                  'reason', d.eval #>> '{review,reason}',
                                  'overlap', d.eval #>> '{review,overlap}',
                                  'belongs_in', d.eval #>> '{review,belongs_in}') END AS reviewer,
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
                   live_use AS (
                       SELECT DISTINCT ON (o.folder) o.folder, jsonb_build_object(
                                  'first_shipped_on', o.first_shipped_on,
                                  'window_start', o.window_start, 'window_end', o.window_end,
                                  'loads', o.loads, 'load_sessions', o.load_sessions,
                                  'loaded_turns', o.loaded_turns, 'judged_turns', o.judged_turns,
                                  'corrected_turns', o.corrected_turns,
                                  'base_judged_turns', o.base_judged_turns,
                                  'base_corrected_turns', o.base_corrected_turns,
                                  'retired_at', o.retired_at, 'retired_reason', o.retired_reason,
                                  'measured_at', o.measured_at) AS numbers
                         FROM memory.skill_live_outcomes o JOIN proj ON proj.id = o.project_id
                        ORDER BY o.folder, o.first_shipped_on DESC)
                   SELECT f.folder AS folder, l.id AS draft_id, l.status AS status,
                          l.review AS review, l.revision AS revision,
                          l.lesson_watermark AS lesson_watermark,
                          l.lesson_count AS lesson_count, l.description AS description,
                          l.created_at AS drafted_at, l.evaluated_at AS evaluated_at,
                          l.edited_at AS edited_at, l.trigger_score AS trigger_score,
                          l.patched_trigger_score AS patched_trigger_score,
                          l.reviewer AS reviewer,
                          l.verdict_reasons AS verdict_reasons,
                          a.id AS approved_draft_id, i.first_written AS first_written,
                          coalesce(i.live, false) AS live,
                          coalesce(i.installs, '[]'::jsonb) AS installs,
                          r.retired_at AS retired_by_user_at,
                          lu.numbers AS live_use
                     FROM folders f
                    CROSS JOIN proj
                     LEFT JOIN latest l ON l.folder = f.folder
                     LEFT JOIN approved a ON a.folder = f.folder
                     LEFT JOIN installs i ON i.folder = f.folder
                     LEFT JOIN live_use lu ON lu.folder = f.folder
                     LEFT JOIN memory.skill_retirements r
                       ON r.project_id = proj.id AND r.folder = f.folder
                    ORDER BY f.folder;
