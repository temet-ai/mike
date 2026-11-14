SELECT id, mode, problem_id, episode_id, session_uuid, state, host, machine_uuid, pid,
       created_at, started_at, deadline_at, ended_at, tool_calls, tokens, exit_code, end_reason
  FROM wright.investigations
 WHERE id = $1
