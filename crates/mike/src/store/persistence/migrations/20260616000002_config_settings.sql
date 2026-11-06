-- Phase 13 / Settings Gate: open-keyspace settings store.
--
-- Companion to `config.feature_toggles` (20260615000003). Where
-- `feature_toggles` holds the six binary toggles in a closed CHECK set,
-- this table holds the *expanded* operator-tunable ProxyConfig surface —
-- all other settings that the central /settings page exposes.
--
-- Design choices:
--   * Open key space — no CHECK constraint on `key`. The set of tunable
--     settings grows over time and is validated Rust-side against
--     `SETTINGS_META` (a compile-time registry). Enforcing the key set in
--     the DB would require an ALTER TABLE ... CHECK each time a new
--     setting is added, introducing migration churn with no safety benefit
--     (unknown keys are rejected by the Rust layer before any INSERT).
--   * No seed rows — absent keys fall back to the compiled `ProxyConfig`
--     default. Writing a row overrides the compiled default; deleting the
--     row restores it. An empty table is a valid production state.
--   * TEXT PRIMARY KEY — the key is a stable dotted path like
--     `steerer.request_timeout_secs`. The key space is small and never
--     joined on from another table, so no surrogate BIGSERIAL is needed.
--   * JSONB NOT NULL for `value` — settings span Bool, Int, Float, String,
--     StringList, and Enum kinds. JSONB encodes all five without a
--     kind-dispatch table and decodes cleanly to Rust serde_json::Value.
--     This is the canonical schemaless-value pattern, not a JSONB
--     shortcut for domain objects: the schema of each key is enforced by
--     the Rust `SETTINGS_META` registry at write time.
--   * updated_at TIMESTAMPTZ NOT NULL DEFAULT now() — recorded on every
--     upsert so the /settings UI can show when each setting was last
--     changed.
--   * No index beyond the PK — point lookups only; the table is tiny
--     (≤ ~50 rows). A secondary index would be wasteful noise.
--   * config schema — same namespace as `config.feature_toggles`.
--     Keeps all global operator-configuration tables out of `rules.`,
--     `memory.`, and `sessions.` which are domain-scoped.

CREATE SCHEMA IF NOT EXISTS config;

CREATE TABLE IF NOT EXISTS config.settings (
    key        text        PRIMARY KEY,
    value      jsonb       NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE config.settings IS
    'Open-keyspace companion to config.feature_toggles. '
    'One row per operator-tunable ProxyConfig field, keyed by dotted '
    'config path (e.g. ''steerer.request_timeout_secs''). '
    'Absent keys fall back to the compiled ProxyConfig default — an empty '
    'table is a valid production state. '
    'Keys are validated Rust-side against SETTINGS_META; the DB imposes '
    'no closed CHECK so the key surface can grow without migration churn. '
    'Written by the /settings UI via upsert; folded onto ProxyConfig at '
    'AppState::new (apply-on-restart fields) or into the live ArcSwap '
    'overlay (session-frozen fields) depending on read-site classification.';

COMMENT ON COLUMN config.settings.key IS
    'Stable dotted config path identifying the setting, e.g. '
    '''steerer.request_timeout_secs'' or ''reminders.max_lessons_to_inject''. '
    'No CHECK constraint — the key space is open and validated in Rust '
    'against SETTINGS_META before any row is written.';

COMMENT ON COLUMN config.settings.value IS
    'JSON-encoded override for the setting named by `key`. '
    'Encodes Bool, Int, Float, String, StringList, and Enum kinds as '
    'their natural JSON representations. The Rust SettingsService '
    'deserialises via serde_json::Value and validates kind/range against '
    'SETTINGS_META before writing.';

COMMENT ON COLUMN config.settings.updated_at IS
    'Last time this setting was written (set by upsert via now()). '
    'Used by the /settings UI to display "last changed" timestamps.';
