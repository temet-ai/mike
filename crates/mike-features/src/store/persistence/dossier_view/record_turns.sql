WITH turns AS (
    SELECT t.id, t.start_request_id, t.created_at, t.closed_at, t.opened_by,
           COALESCE(t.user_prompt, '') AS prompt,
           COALESCE(array_length(t.files_touched, 1), 0)::int AS files_touched
      FROM memory.operator_turns t
     WHERE t.session_id = $1
       AND t.created_at <= $2
),
closed AS (
    SELECT id FROM turns WHERE closed_at <= $2
),
recap_requests AS (
    SELECT c.request_id
      FROM prompt_kinds.classifications c
      JOIN prompt_kinds.kinds k ON k.id = c.kind_id AND k.code = 'session-recap'
     WHERE c.session_id = $1
),
stop AS (
    SELECT f.operator_turn_id AS turn_id, f.stop_request_id,
           (rr.request_id IS NOT NULL) AS is_recap
      FROM closed c
      JOIN memory.turn_facts f ON f.operator_turn_id = c.id
      LEFT JOIN recap_requests rr ON rr.request_id = f.stop_request_id
     WHERE f.stop_request_id IS NOT NULL
),
stop_text AS (
    SELECT s.turn_id,
           string_agg(b.content, E'\n\n' ORDER BY b.seq_ord) AS text
      FROM stop s
      JOIN responses.stream_blocks b ON b.request_id = s.stop_request_id
      JOIN responses.stream_block_types bt ON bt.id = b.block_type_id AND bt.code = 'text'
     WHERE NOT s.is_recap
       AND btrim(b.content) <> ''
     GROUP BY s.turn_id
),
last_text AS (
    SELECT c.id AS turn_id, l.content AS text
      FROM closed c
      LEFT JOIN stop_text s ON s.turn_id = c.id
     CROSS JOIN LATERAL (
           SELECT b.content
             FROM requests.requests r
             JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                                         AND o.code IN ('primary', 'primary_first')
             JOIN responses.stream_blocks b ON b.request_id = r.id
             JOIN responses.stream_block_types bt ON bt.id = b.block_type_id AND bt.code = 'text'
             LEFT JOIN recap_requests rr ON rr.request_id = r.id
            WHERE r.operator_turn_id = c.id
              AND rr.request_id IS NULL
              AND btrim(b.content) <> ''
            ORDER BY r.id DESC, b.seq_ord DESC
            LIMIT 1) l
     WHERE s.turn_id IS NULL
),
uses AS (
    SELECT r.operator_turn_id AS turn_id, u.id AS use_id, u.tool_name,
           u.input ->> 'command' AS command,
           EXISTS (SELECT 1
                     FROM tools.results x
                    WHERE x.tool_use_id = u.id
                      AND x.is_error
                      AND x.created_at <= $2) AS errored
      FROM tools.uses u
      JOIN requests.requests r ON r.id = u.request_id
      JOIN turns t ON t.id = r.operator_turn_id
     WHERE u.session_id = $1
       AND u.created_at <= $2
),
per_tool AS (
    SELECT turn_id, tool_name, count(*)::int AS n,
           count(*) FILTER (WHERE errored)::int AS errors
      FROM uses
     GROUP BY turn_id, tool_name
),
tool_summary AS (
    SELECT turn_id,
           array_agg(tool_name ORDER BY n DESC, tool_name) AS names,
           array_agg(n ORDER BY n DESC, tool_name) AS counts,
           sum(errors)::int AS errors
      FROM per_tool
     GROUP BY turn_id
),
commits AS (
    SELECT turn_id, count(*)::int AS n,
           (array_agg(left(command, 160) ORDER BY use_id))[1:3] AS commands
      FROM uses
     WHERE tool_name = 'Bash'
       AND command ~ '\mgit\s+(-C\s+\S+\s+)?commit\M'
     GROUP BY turn_id
),
pushes AS (
    SELECT turn_id, count(*)::int AS n,
           (array_agg(left(command, 160) ORDER BY use_id))[1:3] AS commands
      FROM uses
     WHERE tool_name = 'Bash'
       AND command ~ '\mgit\s+(-C\s+\S+\s+)?push\M'
     GROUP BY turn_id
)
SELECT t.id AS turn_id,
       t.created_at AS opened_at,
       t.opened_by,
       f.prompt_kind,
       t.prompt,
       COALESCE(st.text, CASE WHEN sp.is_recap THEN lt.text END) AS stop_text,
       CASE WHEN sp.is_recap IS NOT TRUE THEN lt.text END AS last_text,
       f.stop_reason,
       f.aborted,
       f.interrupted,
       f.ended_on,
       f.checks_count,
       f.checks_failed,
       t.files_touched,
       COALESCE(ts.names, '{}'::text[]) AS tool_names,
       COALESCE(ts.counts, '{}'::int[]) AS tool_counts,
       COALESCE(ts.errors, 0) AS tool_errors,
       COALESCE(cm.n, 0) AS commit_count,
       COALESCE(cm.commands, '{}'::text[]) AS commit_commands,
       COALESCE(ps.n, 0) AS push_count,
       COALESCE(ps.commands, '{}'::text[]) AS push_commands
  FROM turns t
  LEFT JOIN memory.turn_facts f ON f.operator_turn_id = t.id AND t.closed_at <= $2
  LEFT JOIN stop sp ON sp.turn_id = t.id
  LEFT JOIN stop_text st ON st.turn_id = t.id
  LEFT JOIN last_text lt ON lt.turn_id = t.id
  LEFT JOIN tool_summary ts ON ts.turn_id = t.id
  LEFT JOIN commits cm ON cm.turn_id = t.id
  LEFT JOIN pushes ps ON ps.turn_id = t.id
 ORDER BY t.start_request_id, t.id
