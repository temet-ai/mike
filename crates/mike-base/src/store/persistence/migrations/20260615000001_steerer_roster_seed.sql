-- Seed the three built-in steerer registry rows.
--
-- Prerequisites already guaranteed by prior migrations:
--   steering.steerers (20260613000002) — the target table
--   steering.authorities lookup rows: supply_context=1, redirect=2, command=3
--   steering.modes lookup rows: augment=1, drive=2
--   agents.agents (20260604000001_baseline) — agent_id FK target
--
-- Pattern: OVERRIDING SYSTEM VALUE is NOT used here because agents.agents.id
-- is a GENERATED ALWAYS AS IDENTITY column and we want the DB to assign
-- natural ids. We use INSERT ... ON CONFLICT (name) DO NOTHING so this
-- migration is safely re-runnable against any DB state.
--
-- Each steerer requires an agents.agents row first (the FK is NOT NULL).
-- All three steerers start with enabled=false — the operator opts in
-- intentionally after reviewing the configuration.

-- ─── agents.agents prerequisite rows ──────────────────────────────────────

-- Resolve the source_id for 'manual' (id=1 from baseline, but resolve
-- defensively via subquery so a future squash cannot break us).
INSERT INTO agents.agents (name, description)
SELECT 'refocus', 'Refocus steerer — assesses and corrects session drift from the thread anchor'
WHERE NOT EXISTS (SELECT 1 FROM agents.agents WHERE name = 'refocus');

INSERT INTO agents.agents (name, description)
SELECT 'reorienter', 'Re-orienter steerer — redirects sessions that have wandered off-topic'
WHERE NOT EXISTS (SELECT 1 FROM agents.agents WHERE name = 'reorienter');

INSERT INTO agents.agents (name, description)
SELECT 'memory-keeper', 'Memory keeper steerer — supplies context from the proxy memory store'
WHERE NOT EXISTS (SELECT 1 FROM agents.agents WHERE name = 'memory-keeper');

-- ─── steering.steerers seed rows ──────────────────────────────────────────
-- `authority_id` and `default_mode_id` are resolved via subquery against the
-- lookup tables so the insert is independent of lookup row order and future
-- numbering changes.

INSERT INTO steering.steerers
    (name, display_name, description, agent_id,
     authority_id, default_mode_id, model, priority, enabled, config)
SELECT
    'refocus',
    'Refocus',
    'Detects session drift from the thread anchor and nudges the session back on track.',
    (SELECT id FROM agents.agents WHERE name = 'refocus' ORDER BY updated_at DESC LIMIT 1),
    (SELECT id FROM steering.authorities WHERE code = 'command'),
    (SELECT id FROM steering.modes WHERE code = 'drive'),
    'google/gemini-2.5-flash-lite',
    1,
    false,
    '{}'::jsonb
WHERE NOT EXISTS (SELECT 1 FROM steering.steerers WHERE name = 'refocus');

INSERT INTO steering.steerers
    (name, display_name, description, agent_id,
     authority_id, default_mode_id, model, priority, enabled, config)
SELECT
    'reorienter',
    'Re-orienter',
    'Redirects sessions that have drifted to off-topic conversations.',
    (SELECT id FROM agents.agents WHERE name = 'reorienter' ORDER BY updated_at DESC LIMIT 1),
    (SELECT id FROM steering.authorities WHERE code = 'redirect'),
    (SELECT id FROM steering.modes WHERE code = 'augment'),
    'google/gemini-2.5-flash-lite',
    2,
    false,
    '{}'::jsonb
WHERE NOT EXISTS (SELECT 1 FROM steering.steerers WHERE name = 'reorienter');

INSERT INTO steering.steerers
    (name, display_name, description, agent_id,
     authority_id, default_mode_id, model, priority, enabled, config)
SELECT
    'memory-keeper',
    'Memory keeper',
    'Supplies relevant context from the proxy memory store into the session.',
    (SELECT id FROM agents.agents WHERE name = 'memory-keeper' ORDER BY updated_at DESC LIMIT 1),
    (SELECT id FROM steering.authorities WHERE code = 'supply_context'),
    (SELECT id FROM steering.modes WHERE code = 'augment'),
    'google/gemini-2.5-flash-lite',
    3,
    false,
    '{}'::jsonb
WHERE NOT EXISTS (SELECT 1 FROM steering.steerers WHERE name = 'memory-keeper');
