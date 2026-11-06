WITH scoped AS (
    SELECT s.id, s.session_uuid, s.title, s.last_status, s.last_seen
      FROM sessions.sessions s
     WHERE s.project_id = 2
       AND (
           s.last_seen >= now() - make_interval(hours => 24)
           OR EXISTS (
               SELECT 1 FROM workspace.session_registrations wr
                WHERE wr.session_id = s.id
           )
       )
),
req_latest AS (
    SELECT DISTINCT ON (r.session_id) r.session_id, r.model
      FROM requests.requests r
      JOIN scoped ss ON ss.id = r.session_id
     WHERE r.model IS NOT NULL
     ORDER BY r.session_id, r.created_at DESC
),
req_counts AS (
    SELECT r.session_id, COUNT(*) AS request_count
      FROM requests.requests r
      JOIN scoped ss ON ss.id = r.session_id
     GROUP BY r.session_id
),
roster AS (
    SELECT DISTINCT ON (wr.session_id) wr.session_id, wr.cwd, wr.worktree
      FROM workspace.session_registrations wr
      JOIN scoped ss ON ss.id = wr.session_id
     ORDER BY wr.session_id, wr.registered_at DESC
),
goals AS (
    SELECT sg.session_id, sg.goal, sg.next_steps
      FROM sessions.session_goals sg
      JOIN scoped ss ON ss.id = sg.session_id
),
subagent AS (
    SELECT DISTINCT ON (ai.session_id) ai.session_id, ag.name AS agent_type
      FROM agents.instances ai
      JOIN agents.agents ag ON ag.id = ai.agent_id
      JOIN scoped ss ON ss.id = ai.session_id
     ORDER BY ai.session_id, ai.created_at DESC
),
outcome AS (
    SELECT DISTINCT ON (v.session_id) v.session_id, o.code AS outcome
      FROM session_judges.verdicts v
      JOIN session_judges.outcomes o ON o.id = v.outcome_id
      JOIN scoped ss ON ss.id = v.session_id
     ORDER BY v.session_id, v.judged_at DESC, v.id DESC
)
SELECT
    ss.id AS session_id, ss.session_uuid, ss.title, ss.last_status, ss.last_seen AS last_activity_at,
    COALESCE(rc.request_count, 0) AS request_count,
    rl.model, ro.cwd, ro.worktree, g.goal, g.next_steps, sa.agent_type, oc.outcome AS session_outcome
FROM scoped ss
LEFT JOIN req_latest rl ON rl.session_id = ss.id
LEFT JOIN req_counts rc ON rc.session_id = ss.id
LEFT JOIN roster ro ON ro.session_id = ss.id
LEFT JOIN goals g ON g.session_id = ss.id
LEFT JOIN subagent sa ON sa.session_id = ss.id
LEFT JOIN outcome oc ON oc.session_id = ss.id
ORDER BY ss.last_seen DESC;
