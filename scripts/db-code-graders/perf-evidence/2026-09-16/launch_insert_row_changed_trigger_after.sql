INSERT INTO launch.launches (
    session_uuid, backend, folder, repo_root, worktree,
    run_id, seat, template_id, model, agent, claude_args,
    prompt_present, brief_mode, brief_path, brief_text,
    requested_placement, split_direction, anchor_ref,
    remote_control, proxy_env
)
VALUES ('f82b9cca-eb8f-4a0d-99a9-12f341b689ac', 'herdr', '/tmp/mike-spawn-verify', '/tmp/mike-spawn-verify', false,
        NULL, NULL, NULL, NULL, NULL, '["--session-id","f82b9cca-eb8f-4a0d-99a9-12f341b689ac"]'::jsonb,
        true, 'inline', NULL, 'say only the word PONG2 and nothing else',
        'tab', NULL, NULL,
        true, true)
RETURNING id
