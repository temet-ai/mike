WITH target AS (
    SELECT unnest($1::bigint[]) AS session_id
),
genuine AS (
    SELECT t.session_id, t.id, t.start_request_id, t.reaction, COALESCE(t.user_prompt, '') AS user_prompt,
           row_number() OVER (PARTITION BY t.session_id ORDER BY t.start_request_id DESC) AS rn,
           count(*) OVER (PARTITION BY t.session_id) AS n
      FROM target g
      JOIN memory.operator_turns t ON t.session_id = g.session_id
      JOIN memory.turn_facts f ON f.operator_turn_id = t.id
      JOIN requests.requests r ON r.id = t.start_request_id
     WHERE f.prompt_kind = 'genuine' AND r.claude_code_agent_id IS NULL AND t.created_at <= $2
),
prior AS (
    SELECT session_id, array_agg(reaction ORDER BY start_request_id) AS reactions
      FROM genuine
     WHERE rn BETWEEN 2 AND 4
     GROUP BY session_id
),
launch_row AS (
    SELECT DISTINCT ON (s.id) s.id AS session_id, l.seat, l.run_id, l.template_id, l.brief_path, l.brief_text,
           l.brief_note_uuid
      FROM target g
      JOIN sessions.sessions s ON s.id = g.session_id
      JOIN launch.launches l ON l.session_uuid = s.session_uuid
     ORDER BY s.id, l.id DESC
),
brief AS (
    SELECT g.session_id,
           bool_and(COALESCE(
               (la.brief_path IS NOT NULL AND position(la.brief_path IN g.user_prompt) > 0)
               OR (la.brief_note_uuid IS NOT NULL
                   AND position(la.brief_note_uuid::text IN g.user_prompt) > 0)
               OR (la.brief_text IS NOT NULL AND g.user_prompt = la.brief_text), false)) AS all_brief
      FROM genuine g
      JOIN launch_row la ON la.session_id = g.session_id
     GROUP BY g.session_id
)
SELECT e.session_id AS "session_id!", e.id AS "end_turn_id!", t.closed_at AS end_turn_closed_at,
       f.next_state AS "next_state!", f.session_class AS "session_class!",
       f.session_class_rule AS "session_class_rule!", f.aborted AS "aborted!",
       f.interrupted AS "interrupted!", f.close_cause AS "close_cause!", f.stop_reason,
       f.ended_on AS "ended_on!", e.n AS "genuine_turns!",
       COALESCE(p.reactions, '{}') AS "prior_reactions!: Vec<Option<String>>",
       la.seat AS "launch_seat?", la.run_id AS "launch_run_id?",
       la.template_id AS "launch_template_id?", la.brief_path AS "brief_path?",
       la.brief_text AS "brief_text?", sg.role AS "goal_role?",
       COALESCE(b.all_brief, false) AS "all_genuine_are_brief!"
  FROM genuine e
  JOIN memory.operator_turns t ON t.id = e.id
  JOIN memory.turn_facts f ON f.operator_turn_id = e.id
  LEFT JOIN prior p ON p.session_id = e.session_id
  LEFT JOIN launch_row la ON la.session_id = e.session_id
  LEFT JOIN brief b ON b.session_id = e.session_id
  LEFT JOIN sessions.session_goals sg ON sg.session_id = e.session_id
 WHERE e.rn = 1
 ORDER BY e.session_id
