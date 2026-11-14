UPDATE workspace.code_findings f
   SET delivered_request_id = d.request_id,
       delivered_via = d.via,
       delivered_at = now()
  FROM unnest($1::bigint[], $2::bigint[], $3::text[]) AS d(finding_id, request_id, via)
 WHERE f.id = d.finding_id
   AND f.delivered_at IS NULL
