-- Content-addressed tool-definition store: mirrors `messages.messages` +
-- `requests.messages` (one distinct-content row + a request-scoped
-- bridge) so a `tools[]` entry's actual JSON schema survives even if
-- `requests.requests.request_body` is ever dropped -- today
-- `requests.tools` records only (request_id, ord, tool_name), so a
-- tool's schema is recoverable ONLY by re-parsing that column.
--
-- `body` stores each tool EXACTLY as it appeared on the wire -- key
-- order, formatting, and `cache_control` (if present) all preserved, no
-- canonicalisation. Measured (500 sampled requests, 37,029 tool
-- declarations, 2026-07-21): hashing the raw wire bytes and hashing a
-- sorted-keys/cache_control-stripped form produce the IDENTICAL 682
-- distinct definitions at the identical 1.84 MB -- canonicalising bought
-- zero extra dedup and would have cost byte-exact reconstruction of
-- `request_body`'s `tools[]`. `cache_control` itself appeared on 0 of
-- the sampled declarations. See `crates/mike/src/store/persistence/
-- tools.rs`'s module doc comment for the full reasoning.
--
-- `body` is bytea, not jsonb, because jsonb REWRITES what it stores:
-- `'{"role":"user","zz":1,"aa":2}'::jsonb` reads back as
-- `{"aa": 2, "zz": 1, "role": "user"}`, reordering keys and reformatting
-- whitespace. A jsonb column therefore cannot hold a wire form at all.
-- Query it with `convert_from(body, 'UTF8')::jsonb` when JSON access is
-- wanted; the stored bytes stay authoritative.
--
-- Measured duplication overall (1% sample, 2026-07-21): 87,904
-- declared-tool rows -> 1,167 distinct definitions; 152 MB embedded ->
-- 3.77 MB deduped. Per-tool dedup (this shape) beats whole-`tools[]`-
-- array dedup ~40x (3% sample: 972 distinct whole-array sets, 405 MB ->
-- 76 MB) -- most duplication is at the individual-tool level (the same
-- builtin Bash/Read/... schemas repeat across unrelated sessions), not
-- the whole declared set.
--
-- `definition_sha256` carries NO index/UNIQUE -- `.claude/rules/sql-set-
-- based.md`'s hard ban on indexing sha columns (random-leaf B-tree
-- bloat). Unlike `messages.messages` (millions of rows, where a
-- per-write existence scan would need that banned index), this table
-- stays small -- low thousands of rows at steady state -- so the write
-- path (`tools::write_request_tools`, `crates/mike/src/store/
-- persistence/tools.rs`) dedups with a plain `WHERE definition_sha256 =
-- ANY($1)` scan of the whole table.
CREATE TABLE IF NOT EXISTS tools.definitions (
    id                bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    definition_sha256 text NOT NULL,
    tool_name         text NOT NULL,
    body              bytea NOT NULL,
    byte_size         integer NOT NULL CHECK (byte_size >= 0),
    first_seen        timestamptz NOT NULL DEFAULT now(),
    last_seen         timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_tools_definitions_last_seen ON tools.definitions USING btree (last_seen DESC);
CREATE INDEX IF NOT EXISTS idx_tools_definitions_tool_name ON tools.definitions USING btree (tool_name);

COMMENT ON TABLE tools.definitions IS 'Content-addressed tool-definition store: one row per DISTINCT tools[] entry, stored exactly as received on the wire (no canonicalisation -- measured identical dedup either way, see tools.rs), deduped GLOBALLY across all sessions (not lineage-scoped like messages.messages). definition_sha256 is deliberately unindexed per the sql-set-based.md hard ban; this table stays small enough for a whole-table scan on write.';

-- requests.tools already IS the per-request declared-tool bridge
-- (request_id, ord, tool_name); this adds the surrogate FK to the
-- content-addressed definition. Nullable: every pre-existing row (all
-- historical captures) has no definition yet -- the operator-run `mike
-- backfill-tool-definitions` subcommand populates them from
-- request_body. RESTRICT (not CASCADE/SET NULL): a definitions row is
-- durable shared reference content with no delete path of its own, so
-- deleting one while a bridge row still points to it is a bug to
-- surface, not silently drop.
ALTER TABLE requests.tools
    ADD COLUMN IF NOT EXISTS definition_id bigint REFERENCES tools.definitions(id) ON DELETE RESTRICT;

CREATE INDEX IF NOT EXISTS idx_request_tools_definition_id ON requests.tools USING btree (definition_id);

COMMENT ON COLUMN requests.tools.definition_id IS 'FK -> tools.definitions(id). NULL until backfilled (mike backfill-tool-definitions) for rows captured before this migration; the live write path always sets it.';
