INSERT INTO wright.fix_batches (day, branch, branches, worktree, machine_uuid)
VALUES ($1::date, $2::text, ARRAY[$2::text], $3::text, $4::uuid)
ON CONFLICT (day) DO UPDATE SET updated_at = now()
RETURNING id, day, branch, branches, worktree, machine_uuid, base_sha, state, rounds, gate_exit,
          gate_output, gate_output_bytes, pr_url, merge_sha, reason, created_at, closed_at,
          landed_at, updated_at
