-- Consolidate alternate agent identities onto their canonical agent row.
--
-- `agents.aliases` was a second relation whose rows had no lifecycle or
-- identity independent of `agents.agents`. Keep the two meanings explicit as
-- typed arrays on the owning row, copy every existing value without changing
-- its spelling, then remove the child table. The legacy singular `alias`
-- column was never used by the application; if an older database nevertheless
-- contains a value, retain it as a name variant before dropping the column.

ALTER TABLE agents.agents
    ADD COLUMN IF NOT EXISTS persona_aliases text[] NOT NULL DEFAULT '{}'::text[],
    ADD COLUMN IF NOT EXISTS name_variants text[] NOT NULL DEFAULT '{}'::text[];

-- Preserve any out-of-band value that may have landed in the unused legacy
-- singular column. The live database has none, but a forward migration should
-- not silently discard a value merely because the current capture does not use
-- the column.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
          FROM information_schema.columns
         WHERE table_schema = 'agents'
           AND table_name = 'agents'
           AND column_name = 'alias'
    ) THEN
        UPDATE agents.agents
           SET name_variants = name_variants || ARRAY[alias]
         WHERE alias IS NOT NULL;
    END IF;
END
$$;

-- Backfill both typed arrays in one grouped pass. `agents.aliases.alias` is
-- NOT NULL and its case-insensitive unique index means the arrays produced
-- here contain no NULLs or case-folded duplicates.
DO $$
BEGIN
    IF to_regclass('agents.aliases') IS NOT NULL THEN
        WITH alias_rollup AS (
            SELECT agent_id,
                   COALESCE(
                       ARRAY_AGG(alias ORDER BY id) FILTER (WHERE kind = 'persona'),
                       '{}'::text[]
                   ) AS persona_aliases,
                   COALESCE(
                       ARRAY_AGG(alias ORDER BY id) FILTER (WHERE kind = 'name_variant'),
                       '{}'::text[]
                   ) AS name_variants
              FROM agents.aliases
             GROUP BY agent_id
        )
        UPDATE agents.agents a
           SET persona_aliases = a.persona_aliases || r.persona_aliases,
               name_variants = a.name_variants || r.name_variants
          FROM alias_rollup r
         WHERE r.agent_id = a.id;
    END IF;
END
$$;

ALTER TABLE agents.agents
    DROP COLUMN IF EXISTS alias,
    DROP COLUMN IF EXISTS frontmatter,
    DROP COLUMN IF EXISTS frontmatter_name,
    DROP COLUMN IF EXISTS frontmatter_description,
    DROP COLUMN IF EXISTS frontmatter_model,
    DROP COLUMN IF EXISTS frontmatter_skills,
    DROP COLUMN IF EXISTS skills;

DROP TABLE IF EXISTS agents.aliases;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'agents.agents'::regclass
           AND conname = 'agents_persona_aliases_no_null'
    ) THEN
        ALTER TABLE agents.agents
            ADD CONSTRAINT agents_persona_aliases_no_null
            CHECK (array_position(persona_aliases, NULL) IS NULL);
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conrelid = 'agents.agents'::regclass
           AND conname = 'agents_name_variants_no_null'
    ) THEN
        ALTER TABLE agents.agents
            ADD CONSTRAINT agents_name_variants_no_null
            CHECK (array_position(name_variants, NULL) IS NULL);
    END IF;
END
$$;

COMMENT ON COLUMN agents.agents.persona_aliases IS
    'Full persona strings that identify this canonical agent. Resolution is case-insensitive in the store.';
COMMENT ON COLUMN agents.agents.name_variants IS
    'Alternate names for this canonical agent, distinct from free-text persona strings. Resolution is case-insensitive in the store.';
