UPDATE workspace.code_findings f
   SET delivered_request_id = d.request_id,
       delivered_at = now()
  FROM unnest($1::bigint[], $2::bigint[]) AS d(finding_id, request_id)
 WHERE f.id = d.finding_id
   AND f.delivered_request_id IS NULL
