WITH shipped(project_uuid, folder, since) AS (VALUES ('02a904fb-0736-4bd9-882c-f1322dcec655'::uuid,'infrastructure-planning','2026-10-14'::date),('5a0ddf4a-05e2-4f76-9056-631adcbb9a30'::uuid,'interchange-clearance','2026-10-14'::date),('73feef04-e53c-45dc-a56a-1ec2aa3e06dc'::uuid,'live-run-verification','2026-10-14'::date),('73feef04-e53c-45dc-a56a-1ec2aa3e06dc'::uuid,'worker-message-delivery','2026-10-14'::date),('82ca7fe5-1a11-42b2-a864-efe67803885a'::uuid,'model-cost-accounting-fleet-coordinator','2026-10-14'::date),('82ca7fe5-1a11-42b2-a864-efe67803885a'::uuid,'fleet-seat-lifecycle-fleet-coordinator','2026-10-14'::date),('82ca7fe5-1a11-42b2-a864-efe67803885a'::uuid,'fleet-verification-fleet-coordinator','2026-10-14'::date),('82ca7fe5-1a11-42b2-a864-efe67803885a'::uuid,'credential-leak-audits','2026-10-14'::date),('82ca7fe5-1a11-42b2-a864-efe67803885a'::uuid,'fleet-verification','2026-10-14'::date),('83e1cf0a-c665-42b4-b16f-a54f39fccac9'::uuid,'fleet-coordination','2026-10-14'::date),('83e1cf0a-c665-42b4-b16f-a54f39fccac9'::uuid,'fleet-verification','2026-10-14'::date),('a51f5007-98fc-4e2a-b1fa-ab5143f3d167'::uuid,'fleet-seat-coordination-fleet-coordinator','2026-10-14'::date),('e51949a9-3797-4725-b47d-c91d339fdbbc'::uuid,'coordinating-model-work','2026-10-14'::date),('e51949a9-3797-4725-b47d-c91d339fdbbc'::uuid,'editing-house-model','2026-10-14'::date),('e51949a9-3797-4725-b47d-c91d339fdbbc'::uuid,'reviewing-house-design','2026-10-14'::date),('e51949a9-3797-4725-b47d-c91d339fdbbc'::uuid,'publishing-design-documents','2026-10-14'::date),('1e4a4c17-3a4c-4244-b532-4c89b304e472'::uuid,'gg-ink-live','2026-09-14'::date),('e68ffca6-cc57-40a0-8e32-f6b99ad7b8f0'::uuid,'lab-draw-model','2026-09-14'::date),('e2cdbbfb-1c1d-425d-9277-03dd2434465d'::uuid,'omarchy','2026-09-14'::date),('4e7efed7-7f6c-4712-bf83-e4f0ec6cf3af'::uuid,'bevy','2026-09-14'::date),('ec5bcb17-7792-47c2-9f7b-c07b1f417f92'::uuid,'plugin-authoring','2026-09-14'::date)),
win AS (
  SELECT s.project_uuid, s.folder, p.id AS project_id,
         greatest(s.since::timestamptz, now() - interval '30 days') AS window_start,
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
SELECT w.project_uuid, w.folder, w.window_start, w.window_end,
       coalesce(l.loads, 0) AS loads, coalesce(l.load_sessions, 0) AS load_sessions,
       coalesce(l.loaded_turns, 0) AS loaded_turns, coalesce(j.judged_turns, 0) AS judged_turns,
       coalesce(j.corrected_turns, 0) AS corrected_turns,
       b.base_judged_turns, b.base_corrected_turns
  FROM win w
  LEFT JOIN loaded l ON l.project_uuid = w.project_uuid AND l.folder = w.folder
  LEFT JOIN judged j ON j.project_uuid = w.project_uuid AND j.folder = w.folder
  JOIN base b ON b.project_uuid = w.project_uuid AND b.folder = w.folder
