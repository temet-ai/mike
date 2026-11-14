WITH refused AS (
    UPDATE wright.actions
       SET state = $2, refusal = $3, updated_at = now()
     WHERE id = $1
       AND state IN ('claimed', 'applied')
    RETURNING id, remedy
)
UPDATE wright.remedies r
   SET consecutive_refusals = r.consecutive_refusals + 1,
       paused_at = CASE WHEN r.paused_at IS NULL AND r.consecutive_refusals + 1 >= $4
                        THEN now() ELSE r.paused_at END,
       pause_reason = CASE WHEN r.paused_at IS NULL AND r.consecutive_refusals + 1 >= $4
                           THEN format('paused after %s refusals in a row; the last, action %s: %s',
                                       r.consecutive_refusals + 1, f.id, $3::text)
                           ELSE r.pause_reason END,
       updated_at = now()
  FROM refused f
 WHERE r.name = f.remedy
RETURNING r.paused_at IS NOT NULL AS paused
