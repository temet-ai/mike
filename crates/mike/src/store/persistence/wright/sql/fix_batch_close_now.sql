UPDATE wright.fix_batches b
   SET state = 'closed', closed_at = now(), updated_at = now()
 WHERE b.id = $1::bigint
   AND b.state = 'open'
   AND NOT EXISTS (
       SELECT 1 FROM wright.repairs r
        WHERE r.batch_id = b.id AND r.state IN ('editing', 'prepared'))
   AND NOT EXISTS (SELECT 1 FROM wright.followups f WHERE f.state = 'handed_off')
   AND (EXISTS (SELECT 1 FROM wright.repairs r WHERE r.batch_id = b.id AND r.state = 'batched')
        OR EXISTS (SELECT 1 FROM wright.followups f WHERE f.batch_id = b.id AND f.state = 'batched'))
RETURNING b.id, b.day, b.branch, b.branches, b.worktree, b.machine_uuid, b.base_sha, b.state,
          b.rounds, b.gate_exit, b.gate_output, b.gate_output_bytes, b.pr_url, b.merge_sha,
          b.reason, b.created_at, b.closed_at, b.landed_at, b.updated_at
