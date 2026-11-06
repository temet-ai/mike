SELECT l.id, l.session_uuid, l.backend, l.backend_reason, l.folder, l.repo_root, l.worktree,
       l.workspace_ref, l.tab_ref, l.surface_ref, l.pid,
       l.run_id, l.seat, l.template_id, l.model, l.agent, l.claude_args,
       l.prompt_present, l.brief_mode, l.brief_path, l.brief_text, l.brief_note_uuid,
       COALESCE(m.consumed_at, rd.at) AS brief_read_at,
       l.requested_placement, l.actual_placement, l.split_direction, l.anchor_ref,
       l.remote_control, l.proxy_env, l.channel_check, l.channel_checked_at,
       l.opened, l.delivered, l.confirmed, l.result, l.reason,
       l.stderr_text, l.stderr_truncated,
       l.created_at, l.opened_at, l.closed_at, l.close_reason, l.close_process_exited
  FROM launch.launches l
  LEFT JOIN mailbox.messages m ON m.message_uuid = l.brief_note_uuid
  LEFT JOIN LATERAL (
        SELECT min(u.created_at) AS at
          FROM tools.uses u
          JOIN sessions.sessions s ON s.id = u.session_id
         WHERE m.consumed_at IS NULL
           AND s.session_uuid = l.session_uuid
           AND u.tool_name = 'Read'
           AND u.file_path = l.brief_path
       ) rd ON true
