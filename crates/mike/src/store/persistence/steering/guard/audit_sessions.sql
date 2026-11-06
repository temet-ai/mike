WITH seat AS (
    SELECT s.id
      FROM sessions.sessions s
      JOIN launch.launches l ON l.session_uuid = s.session_uuid
     WHERE l.run_id IS NOT NULL OR l.seat IS NOT NULL OR l.template_id IS NOT NULL
        OR l.claude_args ? '--fork-session'
),
templated AS (
    SELECT s.id
      FROM sessions.sessions s
      JOIN launch.templates tp ON tp.session_uuid = s.session_uuid
),
named AS (
    SELECT s.id
      FROM sessions.sessions s
      JOIN sessions.session_goals g ON g.session_id = s.id
     WHERE COALESCE(g.role, '') ~* '^(observer|worker|template|probe|J[0-9]+|rust-|db-|frontend-)'
        OR COALESCE(g.display_name, '') ~* '^(observer|worker|template|probe|J[0-9]+|rust-|db-|frontend-)'
),
machine_only AS (
    SELECT t.session_id AS id
      FROM memory.operator_turns t
      JOIN memory.turn_facts f ON f.operator_turn_id = t.id
     WHERE t.created_at >= $1
     GROUP BY t.session_id
    HAVING bool_and(f.prompt_kind <> 'genuine')
)
SELECT id AS "id!" FROM seat
UNION SELECT id FROM templated
UNION SELECT id FROM named
UNION SELECT id FROM machine_only
