SELECT 'repair'::text AS kind, r.id, r.commit_sha, r.finding_id, r.problem_id,
       r.owning_part, NULL::integer AS issue_number,
       format('repair %s: problem %s (%s)', r.id, r.problem_id, p.check_name) AS label
  FROM wright.repairs r
  JOIN wright.problems p ON p.id = r.problem_id
 WHERE r.batch_id = $1::bigint AND r.state = 'batched'
UNION ALL
SELECT 'followup'::text AS kind, f.id, f.batch_commit_sha, NULL::bigint, NULL::bigint,
       f.owning_part, f.issue_number,
       format('follow-up %s: %s', f.id, f.title) AS label
  FROM wright.followups f
 WHERE f.batch_id = $1::bigint AND f.state = 'batched'
 ORDER BY kind DESC, id
