-- Wright follows its own work to its effect.
--
-- * wright.checks.impact: how much a check's problems cost, added to a cause's priority. Work left
--   undone or lost ranks with a fault that stops work, instead of after every older cause. The view
--   also adds ln(1 + amount), where amount is the number of left-undone or failed items the open
--   problems' evidence names.
-- * The repair state 'ineffective': the repair was deployed and its problem came back or never
--   healed. Its finding and follow-ups no longer count as an answer, so the cause is investigated
--   again.
-- * wright.repairable_findings: the findings a repair run may take. A finding whose problem healed
--   before its repair started keeps its repair for 14 days (256 of 597 problems had reopened), and a
--   fix dropped by a conflict with main is rebuilt, up to three attempts.
-- * The check wright_followups also reports issues that wait for a fixer for three days.

SET LOCAL lock_timeout = '10s';

ALTER TABLE wright.checks ADD COLUMN IF NOT EXISTS impact double precision NOT NULL DEFAULT 0
    CHECK (impact >= 0 AND impact <= 100);

UPDATE wright.checks
   SET impact = 8
 WHERE name IN ('judging_left_undone', 'lesson_use_unjudged', 'corrections_without_result',
                'scheduled_job_failures', 'llm_role_errors', 'undelivered_mail', 'request_failures',
                'steering_work_without_reader', 'hook_errors', 'wright_engineer_runs',
                'wright_repairs', 'wright_followups');

UPDATE wright.checks SET impact = 4 WHERE name = 'log_errors';

ALTER TABLE wright.repairs DROP CONSTRAINT IF EXISTS repairs_state_check;
ALTER TABLE wright.repairs ADD CONSTRAINT repairs_state_check
    CHECK (state IN ('editing', 'prepared', 'gating', 'batched', 'dropped', 'gate_failed', 'refused',
                     'landed', 'deployed', 'healthy', 'ineffective'));

ALTER TABLE wright.repairs DROP CONSTRAINT IF EXISTS repairs_ended_has_reason;
ALTER TABLE wright.repairs ADD CONSTRAINT repairs_ended_has_reason
    CHECK (state NOT IN ('gate_failed', 'refused', 'dropped', 'ineffective') OR reason IS NOT NULL);

UPDATE wright.repairs
   SET interrupted = true
 WHERE state = 'dropped'
   AND reason LIKE 'it conflicts with main %';

CREATE OR REPLACE FUNCTION wright.inbox_from_repairs()
RETURNS trigger LANGUAGE plpgsql AS $function$
BEGIN
    INSERT INTO wright.inbox (check_name, problem_id, kind, ref_id, state, summary)
    SELECT p.check_name, n.problem_id, 'repair', n.id, n.state,
           format('repair %s of finding %s (%s) is %s%s', n.id, n.finding_id, n.owning_part, n.state,
                  COALESCE(': ' || left(n.reason, 600), '') || COALESCE(' ' || n.pr_url, ''))
      FROM new_rows n
      JOIN old_rows o ON o.id = n.id
      JOIN wright.problems p ON p.id = n.problem_id
     WHERE o.state IS DISTINCT FROM n.state
       AND n.state IN ('gate_failed', 'refused', 'dropped', 'landed', 'deployed', 'healthy',
                       'ineffective');
    RETURN NULL;
END $function$;

CREATE OR REPLACE VIEW wright.repairable_findings AS
WITH earlier AS (
    SELECT finding_id, count(*) AS tries, bool_and(interrupted) AS all_interrupted
      FROM wright.repairs
     GROUP BY finding_id
),
live AS (
    SELECT DISTINCT problem_id
      FROM wright.repairs
     WHERE state IN ('editing', 'prepared', 'batched', 'landed', 'deployed')
)
SELECT f.id AS finding_id, f.problem_id, btrim(f.owning_part) AS owning_part,
       (l.problem_id IS NOT NULL) AS repair_live
  FROM wright.findings f
  JOIN wright.problems p ON p.id = f.problem_id
  LEFT JOIN earlier e ON e.finding_id = f.id
  LEFT JOIN live l ON l.problem_id = f.problem_id
 WHERE f.kind = 'code_defect'
   AND f.cause_established
   AND btrim(COALESCE(f.owning_part, '')) <> ''
   AND (e.finding_id IS NULL OR (e.all_interrupted AND e.tries < 3))
   AND (p.state = 'open'
        OR (f.created_at >= now() - interval '14 days'
            AND NOT EXISTS (
                SELECT 1
                  FROM wright.repairs r
                 WHERE r.problem_id = f.problem_id
                   AND r.created_at > f.created_at
                   AND r.state IN ('landed', 'deployed', 'healthy'))
            AND NOT EXISTS (
                SELECT 1
                  FROM wright.followup_findings ff
                  JOIN wright.followups fu ON fu.id = ff.followup_id
                 WHERE ff.finding_id = f.id
                   AND fu.kind = 'code_defect'
                   AND fu.state IN ('handed_off', 'batched', 'fixed', 'dropped')
                   AND btrim(COALESCE(fu.owning_part, f.owning_part)) = btrim(f.owning_part))));

CREATE OR REPLACE VIEW wright.cause_queue AS
WITH open_eps AS (
    SELECT p.id AS problem_id, p.cause_id, p.check_name, p.resource, p.opened_at,
           e.id AS episode_id, e.observations, k.impact, a.amount
      FROM wright.problems p
      JOIN wright.episodes e ON e.problem_id = p.id AND e.closed_at IS NULL
      JOIN wright.checks k ON k.name = p.check_name
     CROSS JOIN LATERAL (
         SELECT COALESCE(max((v.value #>> '{}')::float8), 0) AS amount
           FROM jsonb_each(CASE WHEN jsonb_typeof(e.evidence) = 'object'
                                THEN e.evidence ELSE '{}'::jsonb END) v
          WHERE v.key IN ('left_undone', 'lessons', 'failed')
            AND jsonb_typeof(v.value) = 'number'
            AND (v.value #>> '{}')::float8 > 0
     ) a
     WHERE p.state = 'open'
       AND p.cause_id IS NOT NULL
),
answered AS (
    SELECT f.id, f.problem_id, f.episode_id
      FROM wright.findings f
     WHERE NOT EXISTS (
         SELECT 1 FROM wright.repairs r
          WHERE r.finding_id = f.id AND r.state = 'ineffective')
),
found AS (
    SELECT o.cause_id, max(f.id) AS finding_id
      FROM open_eps o
      JOIN answered f ON f.episode_id = o.episode_id
     GROUP BY o.cause_id
),
followed AS (
    SELECT o.cause_id, max(fu.id) AS followup_id, max(fu.issue_number) AS issue_number
      FROM open_eps o
      JOIN answered f ON f.problem_id = o.problem_id
      JOIN wright.followup_findings ff ON ff.finding_id = f.id
      JOIN wright.followups fu ON fu.id = ff.followup_id
     WHERE fu.state IN ('open', 'filed', 'handed_off', 'batched', 'stuck')
     GROUP BY o.cause_id
),
tried AS (
    SELECT o.cause_id,
           count(*) FILTER (WHERE i.state <> 'unavailable') AS attempts,
           max(i.created_at) AS last_attempt_at
      FROM open_eps o
      JOIN wright.investigations i ON i.episode_id = o.episode_id
     GROUP BY o.cause_id
),
grouped AS (
    SELECT o.cause_id,
           count(*) AS problems,
           sum(o.observations) AS observations,
           min(o.opened_at) AS opened_at,
           max(o.impact) AS impact,
           sum(o.amount) AS amount,
           (array_agg(o.problem_id ORDER BY o.observations DESC, o.problem_id))[1] AS problem_id,
           (array_agg(o.episode_id ORDER BY o.observations DESC, o.problem_id))[1] AS episode_id,
           (array_agg(o.check_name ORDER BY o.observations DESC, o.problem_id))[1] AS check_name,
           (array_agg(o.resource ORDER BY o.observations DESC, o.problem_id))[1] AS resource
      FROM open_eps o
     GROUP BY o.cause_id
)
SELECT g.cause_id, g.problem_id, g.episode_id, g.check_name, g.resource,
       g.problems, g.observations, g.opened_at,
       COALESCE(t.attempts, 0) AS attempts, t.last_attempt_at,
       fd.finding_id, fw.followup_id, fw.issue_number,
       wright.has_enabled_remedy(g.check_name) AS remedied,
       s.priority,
       (c.pinned_at IS NOT NULL) AS pinned,
       c.operator_priority,
       COALESCE(c.operator_priority, s.priority) AS rank_priority,
       c.group_id,
       rp.statement,
       g.impact,
       g.amount
  FROM grouped g
  JOIN wright.causes c ON c.id = g.cause_id
  CROSS JOIN LATERAL (
      SELECT round((g.problems + ln(1 + g.observations)
                    + extract(epoch FROM now() - g.opened_at) / 86400.0
                    + g.impact + ln(1 + g.amount))::numeric, 2)::float8 AS priority
  ) s
  LEFT JOIN tried t USING (cause_id)
  LEFT JOIN found fd USING (cause_id)
  LEFT JOIN followed fw USING (cause_id)
  JOIN wright.problems rp ON rp.id = g.problem_id;

UPDATE wright.checks
   SET query = $query$
WITH stuck AS (
    SELECT 'followup'::text AS scope, f.id::text AS resource,
           jsonb_build_object('followup_id', f.id, 'kind', f.kind, 'check_name', f.check_name,
               'title', f.title, 'issue_number', f.issue_number, 'issue_url', f.issue_url,
               'attempts', f.attempts, 'fixer_machine', f.fixer_machine,
               'closed_reason', f.closed_reason, 'updated_at', f.updated_at) AS evidence
      FROM wright.followups f
     WHERE f.state = 'stuck'
       AND f.updated_at >= now() - make_interval(days => ($1->>'window_days')::int)
     ORDER BY f.id DESC
     LIMIT 200
),
waiting AS (
    SELECT f.id, f.issue_number, f.filed_at
      FROM wright.followups f
     WHERE f.state = 'filed'
       AND f.filed_at < now() - make_interval(days => COALESCE(($1->>'waiting_days')::int, 3))
)
SELECT scope, resource, evidence FROM stuck
UNION ALL
SELECT 'followup', 'issues waiting for a fixer',
       jsonb_build_object('left_undone', count(*), 'oldest_filed_at', min(filed_at),
           'issues', jsonb_agg(issue_number ORDER BY filed_at),
           'last_hand_off_at', (SELECT max(handed_off_at) FROM wright.followups))
  FROM waiting
HAVING count(*) > 0
$query$,
       params = params || '{"waiting_days": 3}'::jsonb,
       updated_at = now()
 WHERE name = 'wright_followups';
