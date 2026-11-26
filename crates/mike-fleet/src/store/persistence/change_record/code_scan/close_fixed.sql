WITH kept AS (
    UPDATE workspace.code_findings f
       SET outcome = 'kept',
           outcome_scan_id = $3,
           outcome_at = now(),
           answer = 'kept',
           answer_reason = k.reason,
           answer_ruling = 'Kept by the reason written beside the code.',
           answered_at = now()
      FROM unnest($5::text[], $6::text[]) AS k(fingerprint, reason)
     WHERE f.checkout = $1
       AND f.path = $2
       AND f.outcome IN ('open', 'ignored')
       AND f.severity <> 'error'
       AND f.fingerprint = k.fingerprint
    RETURNING f.id
), closed AS (
    UPDATE workspace.code_findings f
       SET outcome = 'fixed',
           outcome_scan_id = $3,
           outcome_at = now()
     WHERE f.checkout = $1
       AND f.path = $2
       AND f.outcome IN ('open', 'ignored')
       AND NOT (f.fingerprint = ANY($4::text[]))
       AND NOT (f.fingerprint = ANY($5::text[]))
    RETURNING f.id
)
UPDATE workspace.code_scans s
   SET findings_fixed = (SELECT count(*) FROM closed)
 WHERE s.id = $3
RETURNING ARRAY(SELECT id FROM closed UNION ALL SELECT id FROM kept) AS "closed!"
