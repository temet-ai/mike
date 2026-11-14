WITH closed AS (
    UPDATE workspace.code_findings f
       SET outcome = 'fixed',
           outcome_scan_id = $3,
           outcome_at = now()
     WHERE f.checkout = $1
       AND f.path = $2
       AND f.outcome IN ('open', 'ignored')
       AND NOT (f.fingerprint = ANY($4::text[]))
    RETURNING f.id
)
UPDATE workspace.code_scans s
   SET findings_fixed = (SELECT count(*) FROM closed)
 WHERE s.id = $3
RETURNING s.findings_fixed AS "fixed!"
