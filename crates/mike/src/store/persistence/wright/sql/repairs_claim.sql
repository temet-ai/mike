WITH picked AS (
    SELECT DISTINCT ON (problem_id) finding_id, problem_id, owning_part
      FROM wright.repairable_findings
     WHERE NOT repair_live
     ORDER BY problem_id, finding_id
),
named AS (
    SELECT finding_id, problem_id, owning_part
      FROM (SELECT * FROM picked ORDER BY finding_id LIMIT $3) next
),
claimed AS (
    INSERT INTO wright.repairs (finding_id, problem_id, owning_part, state, worktree, branch,
                                branches, machine_uuid, batch_id)
    SELECT n.finding_id, n.problem_id, n.owning_part, 'editing', b.worktree, b.branch,
           ARRAY[b.branch], $2::uuid, b.id
      FROM named n
      JOIN wright.fix_batches b ON b.id = $1::bigint AND b.state = 'open'
    ON CONFLICT DO NOTHING
    RETURNING id, finding_id, problem_id, owning_part, worktree, branch, batch_id
),
earlier AS (
    SELECT r.finding_id, string_agg(format('repair %s ended %s: %s', r.id, r.state, r.reason), E'\n' ORDER BY r.id) AS attempts
      FROM wright.repairs r
      JOIN claimed c ON c.finding_id = r.finding_id AND r.id < c.id
     GROUP BY r.finding_id
)
SELECT c.id, c.finding_id, c.problem_id, c.owning_part, c.worktree, c.branch, c.batch_id,
       p.check_name, f.symptom, f.confirmed_cause, e.attempts AS earlier
  FROM claimed c
  JOIN wright.problems p ON p.id = c.problem_id
  JOIN wright.findings f ON f.id = c.finding_id
  LEFT JOIN earlier e ON e.finding_id = c.finding_id
 ORDER BY c.id
