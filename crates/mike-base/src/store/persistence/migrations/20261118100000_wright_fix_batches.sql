-- Wright's fixes share one branch per UTC day (wright-build thread, 2026-11-18). A repair run
-- and an issue fixer each commit to wright/fixes-YYYY-MM-DD in one worktree on the fix host;
-- after the day ends the branch is gated once by the push hook, opened as one pull request and
-- squash-merged.
--
-- wright.fix_batches: one row per day. States: open (fixes are being committed), closed (the day
-- ended, or the operator closed it; waiting to land), gating (rebased, the push hook and the
-- merge run), refused (the hook refused and no fix could be blamed; held for the operator),
-- landed (merged, merge_sha), failed (could not land; reason). branch is the branch the current
-- gate pushes, branches lists every one used (a rebase after main moved starts a new one, so no
-- push is ever forced). rounds counts engineer rounds spent on a refused gate.
--
-- wright.repairs gains batch_id and the states batched (committed on the batch branch, waiting for
-- the batch to land) and dropped (taken out of the batch; reason). wright.followups gains
-- batch_id, batch_commit_sha and the state batched (the fixer committed on the batch branch; the
-- issue closes at the batch's merge through Fixes #n). wright.followup_limits.fix_host names the
-- machine that holds the batch.

SET LOCAL lock_timeout = '10s';

CREATE TABLE IF NOT EXISTS wright.fix_batches (
    id                bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    day               date        NOT NULL UNIQUE,
    branch            text        NOT NULL,
    branches          text[]      NOT NULL DEFAULT '{}',
    worktree          text        NOT NULL,
    machine_uuid      uuid,
    base_sha          text,
    state             text        NOT NULL DEFAULT 'open'
                                  CHECK (state IN ('open', 'closed', 'gating', 'refused', 'landed', 'failed')),
    rounds            integer     NOT NULL DEFAULT 0 CHECK (rounds >= 0),
    gate_exit         integer,
    gate_output       text,
    gate_output_bytes integer     CHECK (gate_output_bytes >= 0),
    pr_url            text,
    merge_sha         text,
    reason            text,
    created_at        timestamptz NOT NULL DEFAULT now(),
    closed_at         timestamptz,
    landed_at         timestamptz,
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fix_batches_ended_has_reason
        CHECK (state NOT IN ('refused', 'failed') OR reason IS NOT NULL),
    CONSTRAINT fix_batches_landed_has_merge
        CHECK (state <> 'landed' OR (merge_sha IS NOT NULL AND landed_at IS NOT NULL)),
    CONSTRAINT fix_batches_closed_has_time
        CHECK (state = 'open' OR closed_at IS NOT NULL)
);

CREATE INDEX IF NOT EXISTS idx_wright_fix_batches_live
    ON wright.fix_batches (state, day) WHERE state IN ('open', 'closed', 'gating');

ALTER TABLE wright.repairs ADD COLUMN IF NOT EXISTS batch_id bigint REFERENCES wright.fix_batches(id);
CREATE INDEX IF NOT EXISTS idx_wright_repairs_batch ON wright.repairs (batch_id) WHERE batch_id IS NOT NULL;

UPDATE wright.repairs
   SET state = CASE WHEN state = 'editing' THEN 'refused' ELSE 'gate_failed' END,
       interrupted = true,
       reason = COALESCE(reason, 'interrupted: the repair had its own branch before repairs shared one branch a day'),
       updated_at = now()
 WHERE state IN ('editing', 'prepared', 'gating') AND batch_id IS NULL;

ALTER TABLE wright.repairs DROP CONSTRAINT IF EXISTS repairs_state_check;
ALTER TABLE wright.repairs ADD CONSTRAINT repairs_state_check
    CHECK (state IN ('editing', 'prepared', 'gating', 'batched', 'dropped', 'gate_failed',
                     'refused', 'landed', 'deployed', 'healthy'));
ALTER TABLE wright.repairs DROP CONSTRAINT IF EXISTS repairs_ended_has_reason;
ALTER TABLE wright.repairs ADD CONSTRAINT repairs_ended_has_reason
    CHECK (state NOT IN ('gate_failed', 'refused', 'dropped') OR reason IS NOT NULL);

DROP INDEX IF EXISTS wright.wright_repairs_one_live_per_problem;
CREATE UNIQUE INDEX IF NOT EXISTS wright_repairs_one_live_per_problem
    ON wright.repairs (problem_id)
    WHERE state IN ('editing', 'prepared', 'batched', 'landed', 'deployed');

ALTER TABLE wright.followups ADD COLUMN IF NOT EXISTS batch_id bigint REFERENCES wright.fix_batches(id);
ALTER TABLE wright.followups ADD COLUMN IF NOT EXISTS batch_commit_sha text;
ALTER TABLE wright.followups DROP CONSTRAINT IF EXISTS followups_state_check;
ALTER TABLE wright.followups ADD CONSTRAINT followups_state_check
    CHECK (state IN ('open', 'filed', 'handed_off', 'batched', 'fixed', 'dropped', 'stuck'));
ALTER TABLE wright.followups DROP CONSTRAINT IF EXISTS followups_issued_has_number;
ALTER TABLE wright.followups ADD CONSTRAINT followups_issued_has_number
    CHECK (state NOT IN ('filed', 'handed_off', 'batched', 'fixed') OR issue_number IS NOT NULL);
ALTER TABLE wright.followups DROP CONSTRAINT IF EXISTS followups_batched_has_commit;
ALTER TABLE wright.followups ADD CONSTRAINT followups_batched_has_commit
    CHECK (state <> 'batched' OR (batch_id IS NOT NULL AND batch_commit_sha IS NOT NULL));

DROP INDEX IF EXISTS wright.uq_wright_followups_one_live;
CREATE UNIQUE INDEX IF NOT EXISTS uq_wright_followups_one_live
    ON wright.followups (kind, COALESCE(check_name, ''), COALESCE(owning_part, ''))
    WHERE state IN ('open', 'filed', 'handed_off', 'batched', 'stuck');

ALTER TABLE wright.followup_limits
    ADD COLUMN IF NOT EXISTS fix_host text NOT NULL DEFAULT 'omit' CHECK (btrim(fix_host) <> '');

CREATE OR REPLACE VIEW wright.cause_queue AS
WITH open_eps AS (
    SELECT p.id AS problem_id, p.cause_id, p.check_name, p.resource, p.opened_at,
           e.id AS episode_id, e.observations
      FROM wright.problems p
      JOIN wright.episodes e ON e.problem_id = p.id AND e.closed_at IS NULL
     WHERE p.state = 'open'
       AND p.cause_id IS NOT NULL
),
found AS (
    SELECT o.cause_id, max(f.id) AS finding_id
      FROM open_eps o
      JOIN wright.findings f ON f.episode_id = o.episode_id
     GROUP BY o.cause_id
),
followed AS (
    SELECT o.cause_id, max(fu.id) AS followup_id, max(fu.issue_number) AS issue_number
      FROM open_eps o
      JOIN wright.findings f ON f.problem_id = o.problem_id
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
       round((g.problems + ln(1 + g.observations)
              + extract(epoch FROM now() - g.opened_at) / 86400.0)::numeric, 2)::float8 AS priority
  FROM grouped g
  LEFT JOIN tried t USING (cause_id)
  LEFT JOIN found fd USING (cause_id)
  LEFT JOIN followed fw USING (cause_id)
;
