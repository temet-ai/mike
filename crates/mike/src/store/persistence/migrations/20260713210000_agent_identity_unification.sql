-- Agent identity unification (issue tracked in artifacts/threads/agent-registry-rebuild).
--
-- Problem: agents.agents rows were minted from three paths that don't agree
-- on what "identity" means — dispatch capture (subagent_type, the real
-- identity), the now-removed persona auto-capture (the free-text persona
-- SENTENCE from a subagent's system prompt, shared by many unrelated agents:
-- e.g. "assistant for performing a web search tool use" backs 15+ distinct
-- dispatched agents), and compose's find_or_create. This migration makes the
-- dispatch chain (dispatches.agent_id -> agents.agents) the sole identity
-- source, repoints instances/session_prompts onto it, records personas as
-- aliases where they resolve unambiguously, and purges the rows that only
-- ever existed as persona-sentence noise or one-off test/steerer scratch
-- rows the harness left behind.
--
-- Every FK to agents.agents(id), confirmed via pg_constraint against the
-- live capture DB (not just a migration-file grep): agents.dispatches
-- (RESTRICT), agents.instances (SET NULL), steering.steerers (RESTRICT),
-- memory.lessons (SET NULL), overrides.overrides (SET NULL),
-- system_prompts.session_prompts (SET NULL).
--
-- steering.steerers carries a hard RESTRICT FK the brief for this migration
-- did not anticipate. `refocus` / `reorienter` / `memory-keeper` /
-- `false-done` are not persona-sentence junk: they are the deliberately
-- seeded identity rows for the four built-in steerer kinds (migrations
-- 20260615000001, 20260629000001), and `refocus` / `memory-keeper` /
-- `false-done` back currently ENABLED, non-deleted production steerers.
-- `steer-author` likewise backs a live overrides.overrides composition
-- (POST /api/steerers/assist) resolved by name — deleting it would silently
-- break that lookup even though the FK itself only SET NULLs. The purge
-- predicate below therefore protects any agent referenced by steering
-- .steerers or overrides.overrides, on top of the dispatch/instance/
-- session_prompts/alias checks. Only the harness's own dead scratch steerers
-- (soft-deleted, disabled, name-matched) are removed as a companion cleanup
-- so their backing junk agent rows become purgeable.
--
-- Set-based throughout (per .claude/rules/sql-set-based.md): every backfill
-- is an UPDATE ... FROM a GROUP BY/HAVING-resolved CTE or temp table, no
-- correlated scalar subqueries, no sha indexed or matched.

-- ─── 1. New source: builtin (harness sub-agents dispatched with no
--        definition file — Explore/general-purpose/fork/Plan ship inside
--        Claude Code itself) ─────────────────────────────────────────────

INSERT INTO agents.sources (id, code) OVERRIDING SYSTEM VALUE
SELECT 4, 'builtin'
WHERE NOT EXISTS (SELECT 1 FROM agents.sources WHERE id = 4);

-- ─── 2. agents.aliases — alternate names/personas that resolve to a single
--        canonical agents.agents row ────────────────────────────────────

CREATE TABLE IF NOT EXISTS agents.aliases (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    agent_id bigint NOT NULL REFERENCES agents.agents(id) ON DELETE CASCADE,
    alias text NOT NULL,
    kind text NOT NULL CHECK (kind IN ('persona', 'name_variant')),
    created_at timestamptz DEFAULT now() NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_agents_aliases_lower_alias
    ON agents.aliases (LOWER(alias));
CREATE INDEX IF NOT EXISTS idx_agents_aliases_agent_id
    ON agents.aliases (agent_id);

COMMENT ON TABLE agents.aliases IS 'Alternate identity strings (persona sentences, name variants) that resolve to one canonical agents.agents row. Populated by backfill (persona -> single dispatched agent) and, going forward, manual curation.';

-- ─── 3. Reclassify the built-in harness agents that already exist as
--        `observed` rows. Never inserts — the brief this migration serves
--        forbids minting new agents here. ───────────────────────────────

UPDATE agents.agents
   SET source_id = 4
 WHERE LOWER(name) IN ('fork', 'general-purpose', 'explore', 'plan')
   AND source_id <> 4;

-- ─── 4. Backfill agents.instances.agent_id from the dispatch chain — the
--        one FK path that is never ambiguous (a dispatch names exactly one
--        agent). Overwrites any value the old persona-matching path wrote.

UPDATE agents.instances ai
   SET agent_id = d.agent_id
  FROM agents.dispatches d
 WHERE d.id = ai.dispatch_id
   AND ai.agent_id IS DISTINCT FROM d.agent_id;

-- ─── 5a. Repoint session_prompts.agent_id from its (now-correct)
--         instance_id, wherever one is already linked. ────────────────────

UPDATE system_prompts.session_prompts sp
   SET agent_id = ai.agent_id
  FROM agents.instances ai
 WHERE ai.id = sp.instance_id
   AND sp.agent_id IS DISTINCT FROM ai.agent_id;

-- ─── 5b. Link session_prompts rows that never got an instance_id (the old
--         capture path only set agent_id, not instance_id) by matching
--         (session_id, subagent_persona) against agents.instances — but
--         only when every matching instance agrees on one agent. Ambiguous
--         personas (the common case — one persona sentence shared by many
--         distinct agents) are left NULL rather than guessed. ────────────

WITH persona_match AS (
    SELECT sp.id AS session_prompt_id,
           (array_agg(ai.id ORDER BY ai.id DESC))[1] AS instance_id,
           MIN(ai.agent_id) AS agent_id
    FROM system_prompts.session_prompts sp
    JOIN agents.instances ai
      ON ai.session_id = sp.session_id
     AND ai.subagent_persona = sp.subagent_persona
   WHERE sp.instance_id IS NULL
     AND sp.subagent_persona IS NOT NULL
   GROUP BY sp.id
  HAVING COUNT(DISTINCT ai.agent_id) = 1
)
UPDATE system_prompts.session_prompts sp
   SET instance_id = pm.instance_id,
       agent_id = pm.agent_id
  FROM persona_match pm
 WHERE pm.session_prompt_id = sp.id;

-- ─── 6. Companion cleanup: the harness's own dead scratch steerers
--        (delivery/live-proof/crit-proof rigs, an abandoned SHA-named
--        steerer) are soft-deleted and disabled already — remove them by
--        exact name so their backing junk agent rows lose their RESTRICT
--        reference and become purgeable below. Never touches an enabled or
--        non-deleted row, so a live steerer can never be swept here.
--        steering.interventions carries its own RESTRICT FK to steerer_id —
--        these six rows recorded real firings during feature validation
--        (proof-of-concept runs, not production incidents), so their
--        intervention history is cleared alongside the steerer itself,
--        scoped by the identical name list. steering.enrolments has
--        ON DELETE CASCADE and needs no explicit statement. ─────────────

DELETE FROM steering.interventions
 WHERE steerer_id IN (
     SELECT id FROM steering.steerers
      WHERE name IN (
          'steerer-for-f463f95e', 'live-proof-marker', 'live-proof-userprompt',
          'crit4-everyturn-proof', 'crit6-ui-proof', 'delivery-proof'
      )
        AND deleted_at IS NOT NULL
        AND enabled = false
 );

DELETE FROM steering.steerers
 WHERE name IN (
     'steerer-for-f463f95e', 'live-proof-marker', 'live-proof-userprompt',
     'crit4-everyturn-proof', 'crit6-ui-proof', 'delivery-proof'
 )
   AND deleted_at IS NOT NULL
   AND enabled = false;

-- ─── 7. Purge candidates: agents.agents rows with zero remaining
--        references anywhere, computed set-based after every repoint and
--        the companion steerer cleanup above have run. file_backed
--        (source_id=2) and builtin (source_id=4) rows are never candidates
--        regardless of reference counts. Computed before alias seeding so
--        step 8's name-collision check can tell a persona-sentence junk
--        row (about to be purged, so a same-text collision is expected and
--        harmless) from a real surviving agent name. ─────────────────────

CREATE TEMP TABLE _agent_identity_purge_candidates ON COMMIT DROP AS
SELECT a.id
  FROM agents.agents a
 WHERE a.source_id NOT IN (2, 4)
   AND NOT EXISTS (SELECT 1 FROM agents.dispatches d WHERE d.agent_id = a.id)
   AND NOT EXISTS (SELECT 1 FROM agents.instances i WHERE i.agent_id = a.id)
   AND NOT EXISTS (SELECT 1 FROM system_prompts.session_prompts sp WHERE sp.agent_id = a.id)
   AND NOT EXISTS (SELECT 1 FROM agents.aliases al WHERE al.agent_id = a.id)
   AND NOT EXISTS (SELECT 1 FROM steering.steerers st WHERE st.agent_id = a.id)
   AND NOT EXISTS (SELECT 1 FROM overrides.overrides o WHERE o.agent_id = a.id);

-- ─── 8. Seed aliases from persona history: a subagent_persona value that
--        (post step 4) maps to exactly one distinct agent, and isn't
--        already the name of a SURVIVING agent (checked against
--        agents.agents minus the purge candidates — a persona-sentence
--        junk row's own name always self-matches otherwise, since that
--        junk row is exactly where the sentence came from), becomes a
--        persona alias for that agent. ────────────────────────────────

INSERT INTO agents.aliases (agent_id, alias, kind)
SELECT s.agent_id, s.persona, 'persona'
  FROM (
      SELECT ai.subagent_persona AS persona, MIN(ai.agent_id) AS agent_id
        FROM agents.instances ai
       WHERE ai.subagent_persona IS NOT NULL
       GROUP BY ai.subagent_persona
      HAVING COUNT(DISTINCT ai.agent_id) = 1
  ) s
 WHERE NOT EXISTS (
     SELECT 1 FROM agents.agents ag
      WHERE LOWER(ag.name) = LOWER(s.persona)
        AND ag.id NOT IN (SELECT id FROM _agent_identity_purge_candidates)
 )
ON CONFLICT ((LOWER(alias))) DO NOTHING;

-- ─── 9. Null out the one true soft reference (memory.lessons.agent_id is
--        ON DELETE SET NULL and carries no identity-lookup dependents, so a
--        purged agent's lessons keep their text/session/turn context and
--        simply lose the agent attribution). Belt-and-suspenders ahead of
--        the FK's own ON DELETE SET NULL. ────────────────────────────────

UPDATE memory.lessons l
   SET agent_id = NULL
  FROM _agent_identity_purge_candidates c
 WHERE l.agent_id = c.id;

-- ─── 10. Purge. ────────────────────────────────────────────────────────

DELETE FROM agents.agents a
 USING _agent_identity_purge_candidates c
 WHERE a.id = c.id;
