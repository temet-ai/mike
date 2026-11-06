-- Restore the two functional unique indexes that were accidentally dropped
-- during the squash migration (20260604000001_baseline.sql).  They originally
-- lived in 20260430000005_case_insensitive_names.sql (now gone).
--
-- Tests in crates/claude-proxy/tests/integration/case_insensitive_names.rs
-- pin the exact index names:
--   • agents_lower_name_uniqueness_rejects_case_only_duplicate  → uq_agents_lower_name
--   • volatile_lower_name_uniqueness_rejects_case_only_duplicate → uq_volatile_line_patterns_lower_name
--
-- Self-healing: historic captures left case-only (and exact) duplicate rows in
-- agents.agents on some databases (e.g. the dev DB carried 6 + 7 dupes). A bare
-- CREATE UNIQUE INDEX over LOWER(name) would fail on those rows. This migration
-- therefore deduplicates set-based BEFORE creating the indexes, repointing every
-- FK child onto the surviving row (MIN(id) per lower-name group) so no foreign
-- key is orphaned, then deleting the non-survivors. Every statement is
-- idempotent / re-runnable: on a clean DB the survivor CTEs select nothing,
-- the UPDATEs/DELETE touch zero rows, and the indexes are created IF NOT EXISTS.
--
-- Set-based only (per .claude/rules/sql-set-based.md): survivors come from a
-- GROUP BY LOWER(name) CTE; FK repointing is an UPDATE … FROM join against a
-- (non_survivor_id → survivor_id) remap CTE. No correlated scalar subqueries.

-- ─────────────────────────────────────────────────────────────────────
-- agents.agents — dedup, repoint 5 FK children, delete non-survivors
-- ─────────────────────────────────────────────────────────────────────

-- Repoint every FK child from a non-survivor agent id onto its group's
-- survivor (MIN(id) within each LOWER(name) group). The remap CTE is built
-- once per child table and reused. On a clean DB the remap is empty so each
-- UPDATE is a no-op. The 5 FK children (confrelid = agents.agents) confirmed
-- via pg_constraint: agents.dispatches, agents.instances, memory.lessons,
-- overrides.overrides, steering.steerers.

WITH survivors AS (
    SELECT MIN(id) AS survivor_id, LOWER(name) AS lname
    FROM agents.agents
    GROUP BY LOWER(name)
),
remap AS (
    SELECT a.id AS old_id, s.survivor_id
    FROM agents.agents a
    JOIN survivors s ON s.lname = LOWER(a.name)
    WHERE a.id <> s.survivor_id
)
UPDATE agents.dispatches d
   SET agent_id = r.survivor_id
  FROM remap r
 WHERE d.agent_id = r.old_id;

WITH survivors AS (
    SELECT MIN(id) AS survivor_id, LOWER(name) AS lname
    FROM agents.agents
    GROUP BY LOWER(name)
),
remap AS (
    SELECT a.id AS old_id, s.survivor_id
    FROM agents.agents a
    JOIN survivors s ON s.lname = LOWER(a.name)
    WHERE a.id <> s.survivor_id
)
UPDATE agents.instances i
   SET agent_id = r.survivor_id
  FROM remap r
 WHERE i.agent_id = r.old_id;

WITH survivors AS (
    SELECT MIN(id) AS survivor_id, LOWER(name) AS lname
    FROM agents.agents
    GROUP BY LOWER(name)
),
remap AS (
    SELECT a.id AS old_id, s.survivor_id
    FROM agents.agents a
    JOIN survivors s ON s.lname = LOWER(a.name)
    WHERE a.id <> s.survivor_id
)
UPDATE memory.lessons l
   SET agent_id = r.survivor_id
  FROM remap r
 WHERE l.agent_id = r.old_id;

WITH survivors AS (
    SELECT MIN(id) AS survivor_id, LOWER(name) AS lname
    FROM agents.agents
    GROUP BY LOWER(name)
),
remap AS (
    SELECT a.id AS old_id, s.survivor_id
    FROM agents.agents a
    JOIN survivors s ON s.lname = LOWER(a.name)
    WHERE a.id <> s.survivor_id
)
UPDATE overrides.overrides o
   SET agent_id = r.survivor_id
  FROM remap r
 WHERE o.agent_id = r.old_id;

WITH survivors AS (
    SELECT MIN(id) AS survivor_id, LOWER(name) AS lname
    FROM agents.agents
    GROUP BY LOWER(name)
),
remap AS (
    SELECT a.id AS old_id, s.survivor_id
    FROM agents.agents a
    JOIN survivors s ON s.lname = LOWER(a.name)
    WHERE a.id <> s.survivor_id
)
UPDATE steering.steerers st
   SET agent_id = r.survivor_id
  FROM remap r
 WHERE st.agent_id = r.old_id;

-- Delete the non-survivor agent rows now that all children point at survivors.
WITH survivors AS (
    SELECT MIN(id) AS survivor_id, LOWER(name) AS lname
    FROM agents.agents
    GROUP BY LOWER(name)
)
DELETE FROM agents.agents a
 USING survivors s
 WHERE s.lname = LOWER(a.name)
   AND a.id <> s.survivor_id;

-- ─────────────────────────────────────────────────────────────────────
-- rules.volatile_line_patterns — dedup, delete non-survivors
-- (no FK children reference this table; confirmed via pg_constraint where
--  confrelid = rules.volatile_line_patterns — empty. If a future migration
--  adds children, add UPDATE … FROM remap blocks here before the DELETE.)
-- ─────────────────────────────────────────────────────────────────────

WITH survivors AS (
    SELECT MIN(id) AS survivor_id, LOWER(name) AS lname
    FROM rules.volatile_line_patterns
    GROUP BY LOWER(name)
)
DELETE FROM rules.volatile_line_patterns v
 USING survivors s
 WHERE s.lname = LOWER(v.name)
   AND v.id <> s.survivor_id;

-- ─────────────────────────────────────────────────────────────────────
-- Now the functional unique indexes can be created safely.
-- ─────────────────────────────────────────────────────────────────────

-- 1. Case-insensitive uniqueness on agent definitions.
--    Prevents two rows whose names differ only by case
--    (e.g. "frontend-developer" vs "Frontend-Developer").
CREATE UNIQUE INDEX IF NOT EXISTS uq_agents_lower_name
    ON agents.agents (LOWER(name));

-- 2. Case-insensitive uniqueness on volatile line-pattern rules.
--    Same contract as above for rules.volatile_line_patterns.
CREATE UNIQUE INDEX IF NOT EXISTS uq_volatile_line_patterns_lower_name
    ON rules.volatile_line_patterns (LOWER(name));
