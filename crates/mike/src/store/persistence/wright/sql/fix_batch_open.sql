SELECT id, day, branch, branches, worktree, machine_uuid, base_sha, state, rounds, gate_exit,
       gate_output, gate_output_bytes, pr_url, merge_sha, reason, created_at, closed_at,
       landed_at, updated_at
  FROM wright.fix_batches
 WHERE state = 'open'
   AND ($1::date IS NULL OR day = $1::date)
 ORDER BY day DESC, id DESC
 LIMIT 1
