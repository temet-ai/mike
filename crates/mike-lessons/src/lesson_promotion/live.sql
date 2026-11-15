WITH shipped AS (
    SELECT s.project_uuid, s.folder, s.since
      FROM unnest($1::uuid[], $2::text[], $3::date[]) AS s(project_uuid, folder, since)),
win AS (
    SELECT s.project_uuid, s.folder, p.id AS project_id,
           greatest(s.since::timestamptz, now() - make_interval(days => $4)) AS window_start,
           now() AS window_end
      FROM shipped s JOIN projects.projects p ON p.project_uuid = s.project_uuid),
loads AS (
    SELECT w.project_uuid, w.folder, r.session_id, r.operator_turn_id
      FROM win w
      JOIN tools.uses u ON u.tool_name = 'Skill' AND u.input->>'skill' = w.folder
                       AND u.created_at >= w.window_start
      JOIN requests.requests r ON r.id = u.request_id
      LEFT JOIN sessions.sessions ss ON ss.id = r.session_id
     WHERE coalesce(r.project_id, ss.project_id) = w.project_id),
turns AS (
    SELECT t.id, t.project_id, t.created_at, t.reaction
      FROM memory.operator_turns t
      JOIN memory.turn_facts f ON f.operator_turn_id = t.id
     WHERE f.prompt_kind = 'genuine' AND f.text_chars > 0
       AND t.reaction IN ('accepted', 'corrected', 'redirected')
       AND t.created_at >= (SELECT min(window_start) FROM win)),
loaded AS (
    SELECT project_uuid, folder, count(*) AS loads,
           count(DISTINCT session_id) AS load_sessions,
           count(DISTINCT operator_turn_id) AS loaded_turns
      FROM loads GROUP BY project_uuid, folder),
judged AS (
    SELECT x.project_uuid, x.folder, count(*) AS judged_turns,
           count(*) FILTER (WHERE x.reaction = 'corrected') AS corrected_turns
      FROM (SELECT DISTINCT l.project_uuid, l.folder, t.id, t.reaction
              FROM loads l JOIN turns t ON t.id = l.operator_turn_id) x
     GROUP BY x.project_uuid, x.folder),
base AS (
    SELECT w.project_uuid, w.folder, count(t.id) AS base_judged_turns,
           count(t.id) FILTER (WHERE t.reaction = 'corrected') AS base_corrected_turns
      FROM win w LEFT JOIN turns t ON t.project_id = w.project_id AND t.created_at >= w.window_start
     GROUP BY w.project_uuid, w.folder)
SELECT w.project_uuid AS "project_uuid!", w.folder AS "folder!", w.window_start AS "window_start!",
       w.window_end AS "window_end!",
       coalesce(l.loads, 0) AS "loads!", coalesce(l.load_sessions, 0) AS "load_sessions!",
       coalesce(l.loaded_turns, 0) AS "loaded_turns!", coalesce(j.judged_turns, 0) AS "judged_turns!",
       coalesce(j.corrected_turns, 0) AS "corrected_turns!",
       b.base_judged_turns AS "base_judged_turns!", b.base_corrected_turns AS "base_corrected_turns!"
  FROM win w
  LEFT JOIN loaded l ON l.project_uuid = w.project_uuid AND l.folder = w.folder
  LEFT JOIN judged j ON j.project_uuid = w.project_uuid AND j.folder = w.folder
  JOIN base b ON b.project_uuid = w.project_uuid AND b.folder = w.folder
