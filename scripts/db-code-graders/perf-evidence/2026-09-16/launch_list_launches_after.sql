SELECT id, session_uuid, backend, folder, repo_root, worktree,
       workspace_ref, tab_ref, surface_ref, pid,
       run_id, seat, template_id, model, agent, claude_args,
       prompt_present, brief_mode, brief_path, brief_text,
       requested_placement, actual_placement, split_direction, anchor_ref,
       remote_control, proxy_env, channel_check, channel_checked_at,
       opened, delivered, confirmed, result, reason,
       stderr_text, stderr_truncated,
       created_at, opened_at, closed_at, close_reason, close_process_exited
  FROM launch.launches
 WHERE (NULL::text IS NULL OR run_id = NULL::text)
   AND (NOT false::boolean OR closed_at IS NULL)
   AND (NULL::text IS NULL OR backend = NULL::text)
 ORDER BY created_at, id
 LIMIT 50
