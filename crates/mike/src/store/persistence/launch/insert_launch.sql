INSERT INTO launch.launches (
    session_uuid, backend, folder, repo_root, worktree,
    run_id, seat, template_id, model, agent, claude_args,
    prompt_present, brief_mode, brief_path, brief_text,
    requested_placement, split_direction, anchor_ref,
    remote_control, proxy_env, identity, brief_note_uuid, backend_reason
)
VALUES ($1, $2, $3, $4, $5,
        $6, $7, $8, $9, $10, $11,
        $12, $13, $14, $15,
        $16, $17, $18,
        $19, $20, $21, $22, $23)
RETURNING id
