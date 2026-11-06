SELECT id, session_uuid, backend, backend_reason, folder, repo_root, worktree,
       workspace_ref, tab_ref, surface_ref, pid,
       run_id, seat, template_id, model, agent, claude_args,
       prompt_present, brief_mode, brief_path, brief_text, brief_note_uuid,
       COALESCE(
         (SELECT m.consumed_at FROM mailbox.messages m
           WHERE m.message_uuid = launch.launches.brief_note_uuid),
         (SELECT min(u.created_at) FROM tools.uses u
            JOIN sessions.sessions s ON s.id = u.session_id
           WHERE s.session_uuid = launch.launches.session_uuid
             AND u.tool_name = 'Read'
             AND u.file_path = launch.launches.brief_path)) AS "brief_read_at?",
       requested_placement, actual_placement, split_direction, anchor_ref,
       remote_control, proxy_env, channel_check, channel_checked_at,
       opened, delivered, confirmed, result, reason,
       stderr_text, stderr_truncated,
       created_at, opened_at, closed_at, close_reason, close_process_exited
  FROM launch.launches
 WHERE session_uuid = $1
 ORDER BY (closed_at IS NULL) DESC, created_at DESC, id DESC
 LIMIT 1
