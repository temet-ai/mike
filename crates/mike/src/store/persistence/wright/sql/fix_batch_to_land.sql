SELECT id, day, branch, branches, worktree, machine_uuid, base_sha, state, rounds, gate_exit,
       gate_output, gate_output_bytes, pr_url, merge_sha, reason, created_at, closed_at,
       landed_at, updated_at
  FROM wright.fix_batches
 WHERE state IN ('closed', 'gating')
   AND machine_uuid IS NOT DISTINCT FROM $1::uuid
 ORDER BY day, id
