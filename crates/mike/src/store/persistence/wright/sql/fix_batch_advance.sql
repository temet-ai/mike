UPDATE wright.fix_batches
   SET state = COALESCE(NULLIF($2::text, ''), state),
       branch = COALESCE($4::text, branch),
       branches = CASE WHEN $4::text IS NULL OR $4::text = ANY (branches) THEN branches
                       ELSE array_append(branches, $4::text) END,
       base_sha = COALESCE($5::text, base_sha),
       gate_exit = CASE WHEN $6::integer IS NOT NULL THEN $6::integer ELSE gate_exit END,
       gate_output = CASE WHEN $6::integer IS NOT NULL THEN $7::text ELSE gate_output END,
       gate_output_bytes = CASE WHEN $6::integer IS NOT NULL THEN $8::integer
                                ELSE gate_output_bytes END,
       pr_url = COALESCE($9::text, pr_url),
       merge_sha = COALESCE($10::text, merge_sha),
       reason = COALESCE($11::text, reason),
       rounds = rounds + $12::integer,
       closed_at = CASE WHEN $2::text = 'open' THEN closed_at ELSE COALESCE(closed_at, now()) END,
       landed_at = CASE WHEN $2::text = 'landed' THEN now() ELSE landed_at END,
       updated_at = now()
 WHERE id = $1::bigint
   AND state = ANY ($3::text[])
RETURNING id
