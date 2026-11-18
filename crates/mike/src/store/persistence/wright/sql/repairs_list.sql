SELECT id, finding_id, problem_id, owning_part, state, worktree, branch, branches, base_sha, commit_sha,
       gates, patch, gate_exit, gate_output, gate_output_bytes, pr_url, merge_sha, reason,
       machine_uuid, batch_id, created_at, prepared_at, gated_at, landed_at, deployed_at, healthy_at,
       updated_at
  FROM wright.repairs
 WHERE ($1::bigint IS NULL OR problem_id = $1)
   AND ($2::bigint IS NULL OR id < $2)
 ORDER BY id DESC
 LIMIT $3
