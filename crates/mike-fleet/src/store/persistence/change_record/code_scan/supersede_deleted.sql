UPDATE workspace.code_findings f
   SET outcome = 'superseded',
       outcome_at = now()
 WHERE f.checkout = $1
   AND f.path = ANY($2::text[])
   AND f.outcome IN ('open', 'ignored')
