-- Wright triage. Before, every open problem was its own unit of work: the engineer took the
-- oldest one, so one fault raised by 16 decision points or 7 forms of one query cost 16 or 7
-- investigations, and a problem nobody owned held its place in the queue for days.
--
-- wright.causes groups open problems that share a cause. Triage assigns each new problem to a
-- cause after every watch pass: by its resource with numbers and hashes taken out, or, for a
-- check whose params say group_by = run, by the check run that opened it (one run that flags
-- every decision point at once has one cause). The engineer merges causes it finds share its
-- confirmed cause (merged_into). Investigations go to the cause with the highest priority, and
-- a cause that already has a finding or a live follow-up is not investigated again; its new
-- occurrences are commented on the follow-up's issue (commented_at). notified_at records when
-- the operator was last told the cause was waiting.
CREATE TABLE wright.causes (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    check_name text NOT NULL REFERENCES wright.checks(name) ON DELETE CASCADE,
    key text NOT NULL CHECK (btrim(key) <> ''),
    opened_at timestamptz NOT NULL DEFAULT now(),
    merged_into bigint REFERENCES wright.causes(id) ON DELETE SET NULL,
    notified_at timestamptz,
    commented_at timestamptz,
    UNIQUE (check_name, key),
    CHECK (merged_into IS NULL OR merged_into <> id)
);

ALTER TABLE wright.problems ADD COLUMN cause_id bigint REFERENCES wright.causes(id) ON DELETE SET NULL;
CREATE INDEX idx_wright_problems_cause ON wright.problems (cause_id) WHERE cause_id IS NOT NULL;

UPDATE wright.checks
   SET params = params || '{"group_by": "run"}'::jsonb, updated_at = now()
 WHERE name IN ('decisions_health', 'prompt_cache_breaks');

-- tool_contract_errors counts only the tools mike serves, and only refusals by their own
-- handler. A call that Claude Code or the MCP layer refuses against the tool's declared schema
-- (InputValidationError, "Input validation error") is the calling session's mistake: the schema
-- already says what is required, and no change in mike fixes it. Other servers' tools (tether
-- and the rest) are not mike's to fix either.
UPDATE wright.checks
   SET expects = 'No tool mike serves (mike, mike-ask) refuses in its handler a call that its declared schema accepts: a missing, unknown or mistyped field the schema allows is a schema or handler to fix the first time it happens; a rejection is reported for quiet_hours after the newest one. Calls refused against the declared schema, and other servers'' tools, are the caller''s and are not counted',
       query = replace(
                   replace(query,
                           $old$AND u.tool_name LIKE 'mcp\_\_%'$old$,
                           $new$AND u.tool_name ~ '^mcp__(plugin_[a-z0-9_-]+_)?mike(-ask|-mcp)?__'$new$),
                   $old$AND t.body ~* '(invalid arguments|missing field|unknown field|invalid type|InputValidationError|needs `[a-z_]+`|requires `[a-z_]+`)'$old$,
                   $new$AND t.body ~* '(missing field|unknown field|invalid type|needs `[a-z_]+`|requires `[a-z_]+`)'
       AND t.body !~* '(InputValidationError|input validation error|invalid arguments for tool)'$new$),
       updated_at = now()
 WHERE name = 'tool_contract_errors';

-- wright.cause_queue: one row per cause with an open problem, ranked. priority grows with the
-- problems that share the cause, how often they were seen and how long the cause has waited.
-- finding_id or followup_id set means the cause is known and is not investigated again.
CREATE VIEW wright.cause_queue AS
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
     WHERE fu.state IN ('open', 'filed', 'handed_off', 'stuck')
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
