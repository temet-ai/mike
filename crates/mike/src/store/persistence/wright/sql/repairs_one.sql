SELECT id, finding_id, problem_id, owning_part, state, worktree, branch, branches, base_sha, commit_sha,
       gates, patch, gate_exit, gate_output, gate_output_bytes, pr_url, merge_sha, reason,
       machine_uuid, created_at, prepared_at, gated_at, landed_at, deployed_at, healthy_at,
       updated_at
  FROM wright.repairs
 WHERE id = $1
