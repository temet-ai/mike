-- ════════════════════════════════════════════════════════════════════
-- responses.stream_chunks: per-DELTA rows  →  per-BLOCK rows
-- ════════════════════════════════════════════════════════════════════
--
-- The streamed Claude response was stored as one row per raw SSE delta
-- (`responses.stream_chunks`, discriminated by `delta_type_id` ∈
-- text/thinking/input_json via `responses.stream_delta_types`). This
-- migration collapses that to ONE ROW PER ASSEMBLED CONTENT BLOCK:
-- consecutive same-type deltas become a single block row. The verbatim
-- per-delta record is intentionally discarded (operator-approved
-- redesign; see artifacts/threads/streamed-response-chunking/worklog.md).
--
-- Division of labour:
--   * THIS migration   — schema reshape + bounded set-based data migration
--                        coalescing existing delta rows into block rows.
--   * Rust follow-up   — capture write path, readers (rehydrate.rs,
--                        assemble.rs), and the `assembled_body_backfill.rs`
--                        worker that reconstructs `responses.responses.
--                        response_body` from the new block rows. This
--                        migration does NOT touch `response_body`.
--
-- Boundedness: the coalescing INSERT is a single set-based statement over
-- the (existing, finite) `stream_chunks` rows. It is sized by today's
-- captured stream-chunk volume, not unbounded growth, and runs once. It
-- MUST be measured on the real capture DB before merge (no DB in this
-- container — see the handoff Perf-Exempt note). If it ever proves too
-- heavy to run inside the startup migration gate (the 120s startup-gate
-- incident is the cautionary tale), the fallback is to ship the empty
-- block table + a Rust backfill worker that drains the legacy delta rows
-- in batches; the SELECT below is written so it can be lifted verbatim
-- into such a worker with a `LIMIT`/cursor added.

-- ── new lookup table: block types (CHECK/FK pattern, mirrors stream_delta_types) ──
CREATE TABLE responses.stream_block_types (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE,
    CONSTRAINT stream_block_types_code_known
        CHECK (code IN ('text', 'thinking', 'tool_use'))
);
INSERT INTO responses.stream_block_types (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'text'),
    (2, 'thinking'),
    (3, 'tool_use');

COMMENT ON TABLE responses.stream_block_types IS
    'Closed lookup of assembled content-block kinds for responses.stream_blocks: text, thinking, tool_use. Mirrors the CHECK/FK lookup pattern of the legacy responses.stream_delta_types; the input_json delta kind folds into the tool_use block kind.';

-- ── new table: one row per assembled content block ──
CREATE TABLE responses.stream_blocks (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    request_id bigint NOT NULL REFERENCES requests.requests(id) ON DELETE CASCADE,
    session_id bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    block_type_id bigint NOT NULL REFERENCES responses.stream_block_types(id),
    seq_ord integer NOT NULL,
    -- Full block text: the text for `text` blocks, the thinking text for
    -- `thinking` blocks. Empty string for `tool_use` blocks (their data
    -- lives in tools.uses, joined via wire_tool_use_id). NOT NULL so the
    -- column never lies about emptiness; a tool_use block stores ''.
    content text NOT NULL,
    -- Thinking-block signature (captured going forward by the Rust write
    -- path; NULL for tool_use/text blocks and for migrated historical
    -- thinking blocks, whose per-delta record never carried it).
    signature text,
    -- For tool_use blocks: the provider's verbatim tool_use id, so a
    -- reader JOINs tools.uses (its natural key uq_tool_uses_wire_tool_use_id)
    -- for name/input. NULL for text/thinking blocks. Name/input are NOT
    -- denormalized here — id is the only key; never a sha.
    wire_tool_use_id text,
    -- Fingerprint of `content` ONLY. Integrity/display use; deliberately
    -- NOT indexed, NOT unique, NEVER a join/match column (sql-set-based.md).
    content_sha256 text NOT NULL,
    -- Block completion time (max arrived_at of the coalesced delta run for
    -- migrated rows; set when the block's stop event arrives going forward).
    arrived_at timestamptz DEFAULT now() NOT NULL,
    byte_size integer NOT NULL,
    CONSTRAINT stream_blocks_byte_size_nonneg CHECK (byte_size >= 0),
    CONSTRAINT stream_blocks_seq_ord_nonneg CHECK (seq_ord >= 0),
    CONSTRAINT stream_blocks_content_sha256_is_64_hex
        CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    -- Only tool_use blocks may carry a wire id; text/thinking never do.
    -- A tool_use block MAY have a NULL wire id (an orphaned historical run
    -- with no matching tools.uses row at its ordinal — readers tolerate it,
    -- mirroring assemble.rs which omits id/name when the row is absent), so
    -- this forbids a wire id on the wrong block type but does not force one.
    CONSTRAINT stream_blocks_wire_id_only_tool_use CHECK (
        block_type_id = 3 OR wire_tool_use_id IS NULL
    ),
    -- One block per (request, position): the assembler walks seq_ord and
    -- writes once; this makes the write idempotent without a sha and keeps
    -- string_agg reconstruction from ever doubling a block on replay.
    CONSTRAINT uq_stream_blocks_request_seq UNIQUE (request_id, seq_ord)
);

COMMENT ON TABLE responses.stream_blocks IS
    'Per-request assembled streaming content blocks. ONE row per content block (text / thinking / tool_use) — consecutive same-type SSE deltas are coalesced into a single block at capture time. Write-once; full block text in `content`, reconstructed via string_agg. Supersedes the per-delta responses.stream_chunks. response_body reconstruction is the Rust backfill worker''s job, not this table''s.';
COMMENT ON COLUMN responses.stream_blocks.content IS
    'Full block text: the text for `text` blocks, the thinking text for `thinking` blocks; empty string for `tool_use` blocks (data in tools.uses). Never truncated.';
COMMENT ON COLUMN responses.stream_blocks.signature IS
    'Thinking-block signature, captured going forward. NULL for text/tool_use blocks and for historical thinking blocks migrated from the per-delta record (which never carried it).';
COMMENT ON COLUMN responses.stream_blocks.wire_tool_use_id IS
    'For tool_use blocks: the provider''s verbatim tool_use id; readers JOIN tools.uses on it for name/input. NULL otherwise. id is the only key — name/input are not denormalized and no sha is indexed or matched.';
COMMENT ON COLUMN responses.stream_blocks.content_sha256 IS
    'SHA-256 fingerprint of `content` for integrity/display ONLY. Deliberately NOT indexed, NOT unique, NEVER joined/matched. See .claude/rules/sql-set-based.md.';

-- ── indexes: every FK, in this same migration ──
CREATE INDEX idx_response_stream_blocks_request_id
    ON responses.stream_blocks USING btree (request_id);
CREATE INDEX idx_response_stream_blocks_session_id
    ON responses.stream_blocks USING btree (session_id);
CREATE INDEX idx_response_stream_blocks_block_type_id
    ON responses.stream_blocks USING btree (block_type_id);
-- Covering index for the hot reader: per-request, type-filtered, ordered
-- block reads (string_agg of text/thinking blocks). request_id + type +
-- seq_ord ordering, content carried as payload for an index-only scan.
CREATE INDEX idx_response_stream_blocks_request_type_seq
    ON responses.stream_blocks USING btree (request_id, block_type_id, seq_ord);
CREATE INDEX idx_response_stream_blocks_arrived_at
    ON responses.stream_blocks USING brin (arrived_at);

-- ════════════════════════════════════════════════════════════════════
-- DATA MIGRATION — coalesce per-delta rows into per-block rows.
-- ════════════════════════════════════════════════════════════════════
--
-- Set-based and sha-free. Walks the existing stream_chunks once, identifies
-- maximal consecutive same-type runs per request (the classic "islands"
-- pattern via two row_number()s), then aggregates each run into one block.
--
-- Line-by-line on why it is set-based and sha-free:
--   * `runs` CTE: a single ordered pass over stream_chunks. The island id
--     is `seq_ord_rn - per_type_rn` — both are window functions over the
--     same partition, NO correlated subquery, NO per-row lookup.
--   * `blocks` CTE: GROUP BY (request_id, block code, island) with
--     `string_agg(... ORDER BY seq_ord)` to concatenate the run's text in
--     order, and `max(arrived_at)` for block completion time. One scan,
--     fully set-based.
--   * tool_use wire id: resolved by a set-based POSITIONAL JOIN, never by
--     sha. We number each request's tool_use blocks by their block order
--     (`tu_run_rn`) and join to tools.uses numbered by `ord`
--     (`uses_rn`). Equality on (request_id, ordinal) — a small-integer
--     join key, exactly the positional mapping assemble.rs already does in
--     Rust. No content hash is read or matched anywhere.
--   * new seq_ord: `row_number()` over each request's runs ordered by the
--     run's first delta seq_ord — a clean 0-based block ordinal.
--
-- Idempotent: re-running is a no-op because the INSERT is guarded by
-- `NOT EXISTS` against any already-migrated block for the request, and the
-- UNIQUE (request_id, seq_ord) would reject duplicates regardless.
WITH runs AS (
    SELECT
        sc.request_id,
        sc.session_id,
        sc.seq_ord,
        sc.chunk_content,
        sc.arrived_at,
        sdt.code AS delta_code,
        -- island id: rows of one maximal same-type run share one value.
        ROW_NUMBER() OVER (PARTITION BY sc.request_id ORDER BY sc.seq_ord)
          - ROW_NUMBER() OVER (PARTITION BY sc.request_id, sdt.code ORDER BY sc.seq_ord)
          AS island
    FROM responses.stream_chunks sc
    JOIN responses.stream_delta_types sdt ON sdt.id = sc.delta_type_id
    -- Skip requests already migrated, so a re-run does not duplicate work.
    WHERE NOT EXISTS (
        SELECT 1 FROM responses.stream_blocks sb
        WHERE sb.request_id = sc.request_id
    )
),
blocks AS (
    SELECT
        r.request_id,
        -- session_id is constant per request; min() is just an aggregate-safe pick.
        MIN(r.session_id)                                            AS session_id,
        r.delta_code,
        r.island,
        MIN(r.seq_ord)                                               AS first_seq,
        STRING_AGG(r.chunk_content, '' ORDER BY r.seq_ord)          AS content,
        MAX(r.arrived_at)                                           AS arrived_at
    FROM runs r
    GROUP BY r.request_id, r.delta_code, r.island
),
-- New block ordinal per request (0-based), ordered by the run's first delta.
ordered AS (
    SELECT
        b.*,
        ROW_NUMBER() OVER (PARTITION BY b.request_id ORDER BY b.first_seq) - 1
          AS new_seq_ord,
        -- Ordinal of this block among the request's tool_use blocks only,
        -- so it positionally lines up with tools.uses.ord. NULL for
        -- non-tool_use blocks.
        CASE WHEN b.delta_code = 'input_json' THEN
            ROW_NUMBER() OVER (
                PARTITION BY b.request_id, (b.delta_code = 'input_json')
                ORDER BY b.first_seq
            ) - 1
        END AS tu_run_rn
    FROM blocks b
),
-- tools.uses numbered positionally by ord, for the set-based positional join.
uses_numbered AS (
    SELECT
        u.request_id,
        u.wire_tool_use_id,
        ROW_NUMBER() OVER (PARTITION BY u.request_id ORDER BY u.ord) - 1 AS uses_rn
    FROM tools.uses u
)
INSERT INTO responses.stream_blocks
    (request_id, session_id, block_type_id, seq_ord, content, signature,
     wire_tool_use_id, content_sha256, arrived_at, byte_size)
SELECT
    o.request_id,
    o.session_id,
    sbt.id,
    o.new_seq_ord,
    -- tool_use blocks carry no content; text/thinking carry the full run text.
    CASE WHEN o.delta_code = 'input_json' THEN '' ELSE o.content END,
    NULL,  -- signature: never recoverable from the per-delta record.
    -- wire id only for tool_use, resolved positionally (NEVER by sha).
    un.wire_tool_use_id,
    encode(
        digest(
            CASE WHEN o.delta_code = 'input_json' THEN '' ELSE o.content END,
            'sha256'),
        'hex'),
    o.arrived_at,
    octet_length(CASE WHEN o.delta_code = 'input_json' THEN '' ELSE o.content END)
FROM ordered o
JOIN responses.stream_block_types sbt
    ON sbt.code = CASE o.delta_code
                      WHEN 'text'       THEN 'text'
                      WHEN 'thinking'   THEN 'thinking'
                      WHEN 'input_json' THEN 'tool_use'
                  END
LEFT JOIN uses_numbered un
    ON un.request_id = o.request_id
   AND un.uses_rn    = o.tu_run_rn;

-- ════════════════════════════════════════════════════════════════════
-- Rebuild the one DB view that binds to the old table/columns.
-- operator_turn_text_v reads stream_chunks + stream_delta_types directly;
-- repoint it at the new block table. (operator_turn_aggregates_v and
-- session_summary_v do NOT reference stream_chunks and are untouched.)
-- ════════════════════════════════════════════════════════════════════
CREATE OR REPLACE VIEW claude_proxy.operator_turn_text_v AS
SELECT ot.id AS turn_id,
       COALESCE(ot.user_prompt, sr.last_user_message) AS user_prompt,
       CASE WHEN ot.closed_at IS NOT NULL THEN ot.final_assistant_text
            ELSE atx.final_assistant_text END AS final_assistant_text
FROM memory.operator_turns ot
LEFT JOIN requests.requests sr ON sr.id = ot.start_request_id AND ot.user_prompt IS NULL
LEFT JOIN LATERAL (
    SELECT string_agg(b.content, '' ORDER BY b.request_id, b.seq_ord) AS final_assistant_text
    FROM responses.stream_blocks b
    JOIN requests.requests r ON r.id = b.request_id
    JOIN responses.stream_block_types bt ON bt.id = b.block_type_id
    WHERE r.operator_turn_id = ot.id
      AND bt.code = 'text'
      AND ot.closed_at IS NULL
) atx ON true;

-- ════════════════════════════════════════════════════════════════════
-- Drop the superseded per-delta table and its lookup.
-- (The Rust readers/writers are repointed in the follow-up; this migration
-- and that code change land together — see handoff.)
-- ════════════════════════════════════════════════════════════════════
DROP TABLE responses.stream_chunks;
DROP TABLE responses.stream_delta_types;
