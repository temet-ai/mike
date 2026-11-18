SELECT id, day, branch, branches, worktree, machine_uuid, base_sha, state, rounds, gate_exit,
       gate_output, gate_output_bytes, pr_url, merge_sha, reason, created_at, closed_at,
       landed_at, updated_at
  FROM wright.fix_batches
 WHERE ($1::bigint IS NULL OR id < $1::bigint)
 ORDER BY id DESC
 LIMIT $2::bigint
