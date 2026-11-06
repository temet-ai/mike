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
     WHERE l.run_id = 'fleet-ink-migration-2-b870a477' AND l.seat IS NOT NULL AND l.seat <> 'status'
     ORDER BY s.id, l.created_at, l.id
),
bash AS MATERIALIZED (
    SELECT s.seat, u.input->>'command' AS cmd
      FROM seats s
      JOIN tools.uses u ON u.session_id = s.sid AND u.created_at >= s.launched_at AND u.tool_name = 'Bash'
),
runs AS (
    SELECT b.seat, substring(b.cmd from '((scripts/cx|cargo) [^;&|>]*)') AS command
      FROM bash b
     WHERE b.cmd ~ '(^|[;&|(]\s*|\s)(scripts/cx|cargo)\s' AND b.cmd !~ 'git commit'
),
per_command AS (
    SELECT regexp_replace(trim(command), ' [0-9]$', '') AS command, count(*) AS runs, count(DISTINCT seat) AS seats
      FROM runs WHERE command IS NOT NULL GROUP BY 1
)
SELECT coalesce(sum(runs), 0)::bigint AS "commands!",
       coalesce(sum(runs - 1) FILTER (WHERE seats > 1), 0)::bigint AS "repeats!"
  FROM per_command
