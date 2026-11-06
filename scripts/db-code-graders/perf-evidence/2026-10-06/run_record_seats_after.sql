WITH seats AS (
    SELECT DISTINCT ON (s.id) l.seat, s.id AS sid, l.created_at AS launched_at,
           CASE WHEN l.seat LIKE 'coordinator%' THEN 'coordinator'
                WHEN l.seat LIKE 'observer%' THEN 'observer'
                WHEN l.seat LIKE 'acceptance%' THEN 'acceptance'
                WHEN l.seat LIKE 'template-%' THEN 'template'
                WHEN l.seat = 'reference' THEN 'reference'
                ELSE 'worker' END AS kind
      FROM launch.launches l
      JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
     WHERE l.run_id = 'fleet-change-record-2-f9009fa8' AND l.seat IS NOT NULL AND l.seat <> 'status'
     ORDER BY s.id, l.created_at, l.id
),
main_requests AS (
    SELECT s.seat, q.id, q.created_at,
           lag(q.created_at) OVER (PARTITION BY q.session_id ORDER BY q.created_at, q.id) AS prev
      FROM seats s
      JOIN requests.requests q ON q.session_id = s.sid AND q.created_at >= s.launched_at
      JOIN requests.turn_origins o ON o.id = q.turn_origin_id
     WHERE q.claude_code_agent_id IS NULL AND o.code IN ('primary', 'primary_first')
),
activity AS (
    SELECT seat, count(*) AS requests,
           coalesce(sum(extract(epoch FROM created_at - prev)) FILTER (WHERE created_at - prev < interval '3 minutes'), 0) / 60 AS working_minutes,
           coalesce(sum(extract(epoch FROM created_at - prev)) FILTER (WHERE created_at - prev >= interval '3 minutes'), 0) / 60 AS waiting_minutes
      FROM main_requests GROUP BY seat
),
first_request AS (
    SELECT DISTINCT ON (seat) seat, id FROM main_requests ORDER BY seat, created_at, id
),
offered AS (
    SELECT f.seat, array_agg(DISTINCT t.tool_name ORDER BY t.tool_name) AS names
      FROM first_request f JOIN requests.tools t ON t.request_id = f.id
     GROUP BY f.seat
),
calls AS (
    SELECT s.seat, u.tool_name, u.created_at,
           u.input->>'command' AS cmd
      FROM seats s
      JOIN tools.uses u ON u.session_id = s.sid AND u.created_at >= s.launched_at
),
firsts AS (
    SELECT seat, min(created_at) AS first_call,
           min(created_at) FILTER (WHERE tool_name = 'mcp__mike__fleet_commit'
               OR (tool_name = 'Bash' AND cmd ~ 'git\s+(-C\s+\S+\s+)?commit')) AS first_commit
      FROM calls GROUP BY seat
),
totals AS (
    SELECT c.seat, count(*) AS tool_calls,
           count(*) FILTER (WHERE c.tool_name = 'Bash' AND c.cmd ~ '(^|[;&|(]\s*|\s)git\s+(-C\s+\S+\s+)?(--no-pager\s+)?(status|log|diff|show|rev-parse|ls-files|merge-base|fetch|branch|ls-tree|cat-file|rev-list|ls-remote|for-each-ref)\M'
               AND c.cmd !~ '(^|[;&|(]\s*|\s)git\s+(-C\s+\S+\s+)?(add|commit|push|pull|checkout|reset|stash|merge|rebase|apply|rm|mv|cherry-pick|restore|switch|clone|worktree|update-index|update-ref|read-tree|commit-tree|tag|init|clean)\M' AND c.cmd !~ '(cargo|make |scripts/cx|scripts/gate|nextest|pnpm|npm |pytest|sb-test)') AS git_reads,
           count(*) FILTER (WHERE c.tool_name = 'Bash' AND c.cmd ~ '(^|&&|;|\|\||
|do |then )\s*(cd [^;&]+(&&|;)\s*)?(timeout [0-9]+ +)?([A-Z_]+=[^ ]+ +)*([^ ;&]*/)?(scripts/cx|cargo|scripts/gate|scripts/lint-rust|make) ' AND c.cmd !~ 'git commit') AS cargo_commands,
           count(*) FILTER (WHERE f.first_commit IS NOT NULL AND c.created_at <= f.first_commit) AS calls_to_first_commit,
           array_agg(DISTINCT c.tool_name ORDER BY c.tool_name) AS used
      FROM calls c JOIN firsts f ON f.seat = c.seat
     GROUP BY c.seat
)
SELECT s.seat AS "seat", s.kind AS "kind",
       coalesce(a.requests, 0) AS "requests",
       round(coalesce(a.working_minutes, 0), 1)::float8 AS "working_minutes",
       round(coalesce(a.waiting_minutes, 0), 1)::float8 AS "waiting_minutes",
       coalesce(t.tool_calls, 0) AS "tool_calls",
       coalesce(t.git_reads, 0) AS "git_reads",
       coalesce(t.cargo_commands, 0) AS "cargo_commands",
       CASE WHEN f.first_commit IS NULL THEN NULL
            ELSE round(extract(epoch FROM f.first_commit - f.first_call) / 60, 1)::float8 END AS minutes_to_first_commit,
       CASE WHEN f.first_commit IS NULL THEN NULL ELSE t.calls_to_first_commit END AS calls_to_first_commit,
       coalesce(o.names, '{}'::text[]) AS "offered",
       coalesce(t.used, '{}'::text[]) AS "used"
  FROM seats s
  LEFT JOIN activity a ON a.seat = s.seat
  LEFT JOIN firsts f ON f.seat = s.seat
  LEFT JOIN totals t ON t.seat = s.seat
  LEFT JOIN offered o ON o.seat = s.seat
 ORDER BY s.kind, s.seat
