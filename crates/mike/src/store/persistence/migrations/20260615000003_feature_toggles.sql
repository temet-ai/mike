-- Phase 13 / Settings Gate: runtime-mutable feature toggles.
--
-- Introduces the `config` schema as the home for global operator
-- configuration that is not tied to sessions, requests, or filter rules.
-- The six toggles below map 1:1 to feature flags previously baked into
-- the immutable `Arc<ProxyConfig>` and only settable by restarting with
-- a new config.toml.
--
-- Design choices:
--   * `config` schema: clearest semantic fit for singleton/global config
--     tables; keeps them out of `rules.`, `memory.`, and `sessions.`
--     which are domain-scoped.
--   * TEXT PRIMARY KEY: the key space is small, closed, and never joined
--     on from another table. No surrogate BIGSERIAL needed.
--   * BOOLEAN NOT NULL DEFAULT true: all toggles default ON per
--     `.claude/rules/no-default-off.md` — "built but not enabled" is not
--     built. The seed rows also set enabled=true explicitly.
--   * updated_at TIMESTAMPTZ NOT NULL DEFAULT now(): recorded on every
--     upsert so the settings UI can show when a flag was last changed.
--   * CHECK constraint on key: closed set of allowed keys prevents
--     typo-introduced orphan rows from the UI or a future migration.
--   * No index beyond the PK: this table has ≤ 6 rows and is only
--     read at startup + on every settings-page load. PK lookup is
--     sufficient; BRIN or btree would be wasteful noise.

CREATE SCHEMA IF NOT EXISTS config;

CREATE TABLE IF NOT EXISTS config.feature_toggles (
    key        text        PRIMARY KEY
                           CHECK (key IN (
                               'reminders',
                               'recall_injector',
                               'weasel_intercept',
                               'steerer',
                               'lesson_worker',
                               'harvester'
                           )),
    enabled    boolean     NOT NULL DEFAULT true,
    updated_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE config.feature_toggles IS
    'Runtime-mutable feature flags. One row per named toggle; '
    'all default ON (no-default-off rule). '
    'Written by the settings UI via upsert; read at startup and by the '
    'domain service that drives each feature. '
    'The closed CHECK constraint on `key` prevents orphan rows.';

COMMENT ON COLUMN config.feature_toggles.key IS
    'Stable identifier for the feature. Closed set enforced by CHECK.';

COMMENT ON COLUMN config.feature_toggles.enabled IS
    'Whether the feature is currently active. Default true per no-default-off rule.';

COMMENT ON COLUMN config.feature_toggles.updated_at IS
    'Last time the toggle was written (set by upsert via now()). '
    'Used by the settings UI to display "last changed" timestamps.';

-- Seed all six toggles, all enabled=true.
-- ON CONFLICT DO NOTHING makes this idempotent — safe to re-run.
INSERT INTO config.feature_toggles (key, enabled) VALUES
    ('reminders',        true),
    ('recall_injector',  true),
    ('weasel_intercept', true),
    ('steerer',          true),
    ('lesson_worker',    true),
    ('harvester',        true)
ON CONFLICT (key) DO NOTHING;
