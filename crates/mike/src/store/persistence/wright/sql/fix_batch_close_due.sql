WITH due AS (
    SELECT b.id
      FROM wright.fix_batches b
     WHERE b.state = 'open'
       AND b.day < $1::date
       AND NOT EXISTS (
           SELECT 1 FROM wright.repairs r
            WHERE r.batch_id = b.id AND r.state IN ('editing', 'prepared'))
       AND NOT EXISTS (SELECT 1 FROM wright.followups f WHERE f.state = 'handed_off')
),
used AS (
    SELECT r.batch_id FROM wright.repairs r WHERE r.batch_id IN (SELECT id FROM due)
    UNION
    SELECT f.batch_id FROM wright.followups f WHERE f.batch_id IN (SELECT id FROM due)
),
closed AS (
    UPDATE wright.fix_batches
       SET state = 'closed', closed_at = now(), updated_at = now()
     WHERE id IN (SELECT batch_id FROM used)
    RETURNING id, worktree, branch
),
gone AS (
    DELETE FROM wright.fix_batches
     WHERE id IN (SELECT id FROM due)
       AND id NOT IN (SELECT batch_id FROM used)
    RETURNING id, worktree, branch
)
SELECT id, worktree, branch, true AS kept FROM closed
UNION ALL
SELECT id, worktree, branch, false AS kept FROM gone
ORDER BY id
