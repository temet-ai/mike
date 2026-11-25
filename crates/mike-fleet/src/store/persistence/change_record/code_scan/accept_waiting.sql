UPDATE workspace.code_findings f
   SET answer = 'kept',
       answer_ruling = 'accepted by the operator',
       outcome = 'kept',
       outcome_at = now()
  FROM workspace.code_findings one
 WHERE one.id = $1
   AND one.answer = 'waiting'
   AND f.checkout = one.checkout
   AND f.path = one.path
   AND f.rule_id = one.rule_id
   AND f.answered_at = one.answered_at
   AND f.answer = 'waiting'
   AND f.outcome = 'open'
RETURNING f.id
