-- The operator acts on Wright's triage queue (2026-11-24): pins causes, sets their order, merges
-- causes by hand and separates a hand merge again, and sorts causes into named groups.
--
-- wright.causes gains pinned_at (null means not pinned; pinned causes come before every unpinned
-- one), operator_priority (null means the formula decides; when set it replaces the formula's score
-- for that cause) and group_id (a named group, which only changes how the page shows the cause).
--
-- wright.cause_merges records each merge, one row per absorbed cause, with the problem ids that
-- moved from it, so a merge made by hand can be undone. by says who merged: the engineer (a finding's
-- same_cause_as) or the operator. Only an operator merge can be separated; separated_at records when.
--
-- wright.cause_queue keeps its columns and gains pinned, operator_priority, rank_priority
-- (COALESCE(operator_priority, priority), the value the readers order by after pinned) and group_id
-- at the end.

SET LOCAL lock_timeout = '10s';

CREATE TABLE IF NOT EXISTS wright.cause_groups (
    id         bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name       text        NOT NULL UNIQUE CHECK (btrim(name) <> ''),
    created_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE wright.causes ADD COLUMN IF NOT EXISTS pinned_at timestamptz;
ALTER TABLE wright.causes ADD COLUMN IF NOT EXISTS operator_priority double precision
    CONSTRAINT causes_operator_priority_range CHECK (operator_priority BETWEEN -1e12 AND 1e12);
ALTER TABLE wright.causes ADD COLUMN IF NOT EXISTS group_id bigint
    REFERENCES wright.cause_groups(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_wright_causes_group
    ON wright.causes (group_id) WHERE group_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS wright.cause_merges (
    id                bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    root_cause_id     bigint      NOT NULL REFERENCES wright.causes(id) ON DELETE CASCADE,
    absorbed_cause_id bigint      NOT NULL REFERENCES wright.causes(id) ON DELETE CASCADE,
    problem_ids       bigint[]    NOT NULL,
    by                text        NOT NULL CHECK (by IN ('operator', 'engineer')),
    created_at        timestamptz NOT NULL DEFAULT now(),
    separated_at      timestamptz,
    CHECK (root_cause_id <> absorbed_cause_id)
);

CREATE INDEX IF NOT EXISTS idx_wright_cause_merges_root
    ON wright.cause_merges (root_cause_id, id DESC) WHERE separated_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_wright_cause_merges_absorbed
    ON wright.cause_merges (absorbed_cause_id);

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
       s.priority,
       (c.pinned_at IS NOT NULL) AS pinned,
       c.operator_priority,
       COALESCE(c.operator_priority, s.priority) AS rank_priority,
       c.group_id
  FROM grouped g
  JOIN wright.causes c ON c.id = g.cause_id
  CROSS JOIN LATERAL (
      SELECT round((g.problems + ln(1 + g.observations)
                    + extract(epoch FROM now() - g.opened_at) / 86400.0)::numeric, 2)::float8 AS priority
  ) s
  LEFT JOIN tried t USING (cause_id)
  LEFT JOIN found fd USING (cause_id)
  LEFT JOIN followed fw USING (cause_id)
;
