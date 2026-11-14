WITH verified AS (
    UPDATE wright.actions
       SET state = 'verified', effect = $2, verified_at = now(), updated_at = now(),
           applied_at = coalesce(applied_at, now())
     WHERE id = $1
       AND state IN ('claimed', 'applied')
    RETURNING remedy
)
UPDATE wright.remedies r
   SET consecutive_refusals = 0, updated_at = now()
  FROM verified v
 WHERE r.name = v.remedy
RETURNING r.name
