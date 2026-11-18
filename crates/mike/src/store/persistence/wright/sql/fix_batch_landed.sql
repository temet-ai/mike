WITH b AS (
    UPDATE wright.fix_batches
       SET state = 'landed', merge_sha = $2::text, pr_url = COALESCE($3::text, pr_url),
           landed_at = now(), closed_at = COALESCE(closed_at, now()), updated_at = now()
     WHERE id = $1::bigint AND state IN ('closed', 'gating')
    RETURNING id
),
r AS (
    UPDATE wright.repairs r
       SET state = 'landed', merge_sha = $2::text, landed_at = now(), updated_at = now()
      FROM b
     WHERE r.batch_id = b.id AND r.state = 'batched'
    RETURNING r.id
)
SELECT b.id AS batch_id, r.id AS repair_id
  FROM b
  LEFT JOIN r ON true
