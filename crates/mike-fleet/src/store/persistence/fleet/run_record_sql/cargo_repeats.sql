WITH seats AS (
    SELECT rs.seat, rs.session_id AS sid, rs.launched_at, rs.kind
      FROM launch.run_seats rs
     WHERE rs.run_id = $1
),
bash AS MATERIALIZED (
    SELECT s.seat, u.input->>'command' AS cmd
      FROM seats s
      JOIN tools.uses u ON u.session_id = s.sid AND u.created_at >= s.launched_at AND u.tool_name = 'Bash'
),
runs AS (
    SELECT b.seat, b.cmd AS command
      FROM bash b
     WHERE b.cmd ~ $2 AND b.cmd !~ 'git commit'
),
per_command AS (
    SELECT trim(command) AS command, count(*) AS runs, count(DISTINCT seat) AS seats
      FROM runs WHERE command IS NOT NULL GROUP BY 1
)
SELECT coalesce(sum(runs), 0)::bigint AS "commands!",
       coalesce(sum(runs - 1) FILTER (WHERE seats > 1), 0)::bigint AS "repeats!"
  FROM per_command
