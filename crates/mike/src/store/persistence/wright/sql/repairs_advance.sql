UPDATE wright.repairs
   SET state = $2,
       branch = COALESCE($3::text, branch),
       branches = CASE WHEN $3::text IS NULL OR $3::text = ANY (branches) THEN branches
                       ELSE array_append(branches, $3::text) END,
       base_sha = COALESCE($4::text, base_sha),
       commit_sha = COALESCE($5::text, commit_sha),
       patch = COALESCE($6::jsonb, patch),
       gates = gates + CASE WHEN $2 = 'gating' AND state <> 'gating' THEN 1 ELSE 0 END,
       gate_exit = CASE WHEN $7::integer IS NOT NULL THEN $7::integer
                        WHEN $2 = 'gating' AND state <> 'gating' THEN NULL ELSE gate_exit END,
       gate_output = CASE WHEN $7::integer IS NOT NULL THEN $8::text
                          WHEN $2 = 'gating' AND state <> 'gating' THEN NULL ELSE gate_output END,
       gate_output_bytes = CASE WHEN $7::integer IS NOT NULL THEN $9::integer
                                WHEN $2 = 'gating' AND state <> 'gating' THEN NULL
                                ELSE gate_output_bytes END,
       gated_at = CASE WHEN $7::integer IS NOT NULL THEN now()
                       WHEN $2 = 'gating' AND state <> 'gating' THEN NULL ELSE gated_at END,
       pr_url = CASE WHEN $10::text IS NOT NULL THEN $10::text
                     WHEN $2 = 'gating' AND state <> 'gating' THEN NULL ELSE pr_url END,
       merge_sha = COALESCE($11::text, merge_sha),
       reason = COALESCE($12::text, reason),
       prepared_at = CASE WHEN $2 = 'prepared' THEN now() ELSE prepared_at END,
       landed_at = CASE WHEN $2 = 'landed' THEN now() ELSE landed_at END,
       updated_at = now()
 WHERE id = $1
   AND state = ANY ($13::text[])
RETURNING id
