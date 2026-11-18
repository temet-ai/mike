WITH repair_owned AS (
    SELECT ff.followup_id
      FROM wright.followup_findings ff
      JOIN wright.followups fu ON fu.id = ff.followup_id AND fu.kind = 'code_defect'
      JOIN wright.findings fd ON fd.id = ff.finding_id
      LEFT JOIN wright.problems p ON p.id = fd.problem_id
      LEFT JOIN wright.repairs r ON r.finding_id = fd.id
     WHERE (r.id IS NOT NULL
            AND (r.state NOT IN ('gate_failed', 'refused') OR r.interrupted)
            AND btrim(COALESCE(fu.owning_part, r.owning_part)) = btrim(r.owning_part))
        OR (r.id IS NULL
            AND p.state = 'open'
            AND fd.kind = 'code_defect'
            AND fd.cause_established
            AND btrim(COALESCE(fd.owning_part, '')) <> ''
            AND btrim(COALESCE(fu.owning_part, '')) = btrim(fd.owning_part))
),
answered_root_causes AS (
    SELECT root.followup_id
      FROM wright.followup_findings root
      JOIN wright.followups rf ON rf.id = root.followup_id AND rf.kind = 'root_cause'
      JOIN wright.followup_findings child
        ON child.finding_id = root.finding_id AND child.followup_id <> root.followup_id
)
SELECT f.id, f.kind, f.check_name, f.owning_part, f.title, f.body, f.state, f.issue_number, f.issue_url,
       f.attempts, f.fixer_session, f.fixer_machine, f.filed_at, f.handed_off_at, f.closed_at,
       f.closed_reason, f.created_at, f.updated_at
  FROM wright.followups f
 WHERE f.state = 'filed'
   AND f.id NOT IN (SELECT followup_id FROM repair_owned)
   AND f.id NOT IN (SELECT followup_id FROM answered_root_causes)
 ORDER BY f.filed_at, f.id
 LIMIT 1
