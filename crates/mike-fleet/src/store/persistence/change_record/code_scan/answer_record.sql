UPDATE workspace.code_findings f
   SET answer = $2,
       answer_reason = $3,
       answer_ruling = $4,
       answered_at = now(),
       answer_refusals = f.answer_refusals + CASE WHEN $2 = 'refused' THEN 1 ELSE 0 END,
       outcome = CASE WHEN $2 IN ('kept', 'not_mine') THEN $2
                      WHEN $2 = 'waiting' THEN 'open'
                      ELSE f.outcome END,
       outcome_at = CASE WHEN $2 IN ('kept', 'not_mine') THEN now() ELSE f.outcome_at END
 WHERE f.id = ANY($1::bigint[])
   AND f.outcome IN ('open', 'ignored')
