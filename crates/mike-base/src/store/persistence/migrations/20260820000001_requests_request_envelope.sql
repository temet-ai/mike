-- GitHub issue #68 — capture the request envelope as its own fact.
--
-- Every field of the wire body that is NOT `messages` or `tools` has,
-- until now, existed in exactly one place: the `request_body` blob.
-- `requests.requests.params` looks like the structured projection of the
-- envelope but keeps only five fields — `max_tokens`, `temperature`,
-- `thinking_mode`, `thinking_budget`, `thinking_enabled`. Measured over
-- 300 recent captures, the wire also carries `metadata` (299),
-- `output_config` (269), `stream` (269), `context_management` (263) and
-- `diagnostics` (224), none of which any column holds.
--
-- Nothing is lost today, because the raw body is kept. But #68 exists to
-- stop keeping it: `requests.requests` is 80GB across 82,070 rows, and
-- `messages` alone is 83% of those bytes with `tools` a further 14%.
-- Both are already fully normalized (`requests.messages` →
-- `messages.messages`, `requests.tools` → `tools.definitions`). The
-- envelope is not normalized anywhere, so clearing bodies would destroy
-- it.
--
-- `request_envelope` is the body with ONLY those two arrays' contents
-- removed — `"messages":[]`, `"tools":[]` — and every other byte left
-- exactly as the client sent it, key order and all. It is `bytea` rather
-- than `jsonb` deliberately: `jsonb` reorders object keys and drops
-- whitespace, which is precisely the fidelity this column exists to
-- keep. (That normalization is also why a rebuilt `messages` array can
-- only be semantically identical to the wire, never byte-identical —
-- `messages.messages.body` is `jsonb`.)
--
-- Nullable, no backfill here. Historical rows still hold their full
-- body, so their envelope is intact where it always was; backfilling
-- 82,070 rows means reading 80GB and belongs in an explicit CLI pass,
-- not in a migration that runs on every proxy startup. Compaction (#68
-- step 7, still unauthorized) must refuse any row whose envelope is
-- NULL.
--
-- Metadata-only: adding a nullable column with no default does not
-- rewrite the table.

ALTER TABLE requests.requests ADD COLUMN IF NOT EXISTS request_envelope bytea;

COMMENT ON COLUMN requests.requests.request_envelope IS
    'Issue #68: the wire request body with only the `messages` and `tools` '
    'array contents removed, byte-exact otherwise. The envelope fields '
    '(metadata, context_management, output_config, diagnostics, stream, '
    'system, model, max_tokens, thinking) live nowhere else in normalized '
    'form, so this is what makes clearing `request_body` non-destructive. '
    'NULL on rows captured before this column existed and on rows whose '
    'body could not be parsed; compaction must skip those.';
