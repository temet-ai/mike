# Normalize the tool-use data plane: kill the JSONB adapter, move `tool_names` to a companion table, wire call↔result pairing

> **Branch:** `refactor/tool-uses-normalized-rehydrate` — dedicated to this work only.
> This document is the branch's plan of record; commits implement the sections below
> in order. Scratch copy: `~/.claude/plans/deep-rolling-wigderson.md`.

## Context

`crates/claude-proxy/src/store/persistence/rehydrate.rs` still carries two JSONB
smells in the tool-use read path:

1. **`requests.tool_names`** is a live `JSONB NOT NULL` column (GIN-indexed, app-unused
   index) holding tools *declared* in the request body (`body.tools[]`, a JSONB array of
   **bare strings**), semantically distinct from tools *invoked* (`tool_uses`). It is the
   only genuine JSONB-as-storage left in the tool plane. Every other split-out field
   (`request_responses`, `request_system_prompts`, `request_diagnostics`) already moved
   to a companion table; `tool_names` never finished that migration.
2. **`tool_calls`** storage was normalized into `tool_uses` (2026-05-05 cutover), but the
   read path re-wraps `tool_uses` back into the old JSONB shape via
   `jsonb_agg(jsonb_build_object(...))` → `serde_json::Value` → `ToolCallDto` →
   `ToolCall` at four SQL sites — a JSONB read adapter the inspection-fidelity rule names
   as an anti-pattern.

Rehydrate also discards tool_results and fabricates `complete: true` in SQL, so a
rehydrated request shows calls with no results and a faked completion flag. The operator
wants the call↔result linkage surfaced ("here is a tool call, here is its result").

Outcome: `tool_names` promoted to a bridge table following the `request_diagnostics`
house pattern; the JSONB read adapter deleted in favour of typed reads; an additive
paired endpoint that joins each emitted call to its result over the **existing** FK
(`tool_results.tool_use_id → tool_uses.tool_id`, shipped 2026-05-05).

## Decisions (locked)

- **Pairing is a read-surface gap, not a schema gap.** The FK already models the
  relationship on disk; `get_request_tool_results` returns results only (caller must
  cross-ref by hand). The operator chose a **new additive endpoint + MCP tool**. This
  means **zero DB-layer work for pairing** — it is 100% API-layer, additive, no wire
  break. Rehydrate stays single-query; `row_to_intercepted` keeps
  `tool_results: Vec::new()` / `tool_results_in_body: Vec::new()` (a request's
  `tool_results` means "blocks it carried inbound"; a call's result lives in a *later*
  request — pairing is by `tool_use_id`, never shared `request_id`).
- **DB schema follows db-dev's audited design** (§1) — surrogate identity PK, named
  constraints, `created_at`, `COMMENT ON`, `IF NOT EXISTS` indexes, matching the newest
  `request_responses` / `request_diagnostics` precedents.
- **Two migrations, split per the `request_responses` precedent**: A (create + backfill,
  non-destructive) and B (drop column + GIN index). B is gated on the rehydrate read
  path being migrated off `r.tool_names` first.
- `RequestSummary` / `RequestDetail` JSON wire shape **unchanged** (byte-identical); the
  in-memory `From<&InterceptedRequest>` path is the parity oracle.
- `ToolCall.complete` stays on the wire, set as a documented Rust literal `true`
  (a persisted `tool_use` is fully captured), not a SQL fabrication.
- New endpoint exposes `input` as structured JSONB (`serde_json::Value`), full bytes,
  no double-encoding. `RequestDetail.tool_calls` keeps `input_json: String` unchanged.

## Semantic guardrail

`request_tools.tool_name` (declared) and `tool_uses.tool_name` (invoked) share a column
name and similar keys. Wiring the wrong one into `row_to_intercepted.tool_names` vs
`tool_calls` is the correctness trap. The in-memory unit test
`handlers.rs:6583 summary_from_copies_tool_names_preserving_order` must stay green
untouched, plus a new declared-vs-invoked DB test.

## 1. Schema — TWO migrations (db-dev audited)

### Migration A — `…/migrations/20260518000001_request_tools_table.sql` (non-destructive)

```sql
-- Promote requests.tool_names JSONB → typed, indexable bridge table.
-- Mirrors the request_diagnostics promotion (20260502000001). tool_names
-- is the array of tools DECLARED in the request body (body.tools[].name,
-- declaration order significant) — semantically DISTINCT from
-- claude_proxy.tool_uses (tools INVOKED). Non-destructive: requests.tool_names
-- + its GIN index drop in a SEPARATE follow-up (cf. request_responses
-- 20260517000001 → 20260517000002 split). Child table is NOT partitioned —
-- same as every sibling child; composite FK carries the partition column.

CREATE TABLE IF NOT EXISTS claude_proxy.request_tools (
    tool_decl_id        BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    request_id          BIGINT      NOT NULL,
    request_created_at  TIMESTAMPTZ NOT NULL,
    ord                 INT         NOT NULL,
    tool_name           TEXT        NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT request_tools_request_ord_uniq
        UNIQUE (request_id, request_created_at, ord),
    CONSTRAINT request_tools_request_fkey
        FOREIGN KEY (request_id, request_created_at)
        REFERENCES claude_proxy.requests (id, created_at) ON DELETE CASCADE
);

COMMENT ON TABLE claude_proxy.request_tools IS
    'One row per tool DECLARED in a captured request body (body.tools[].name), '
    'preserving declaration order via `ord` (0-based). Promotes the '
    'claude_proxy.requests.tool_names JSONB array to a typed bridge table — '
    'the request_diagnostics pattern. Semantically distinct from '
    'claude_proxy.tool_uses (tools INVOKED). Inspection-fidelity: tool_name is '
    'the FULL declared name, no truncation. See '
    'migrations/20260518000001_request_tools_table.sql.';

CREATE INDEX IF NOT EXISTS idx_request_tools_name
    ON claude_proxy.request_tools (tool_name);
CREATE INDEX IF NOT EXISTS idx_request_tools_request
    ON claude_proxy.request_tools (request_id, request_created_at);

-- Backfill: tool_names is an array of BARE STRINGS → jsonb_array_elements_text
-- (NOT jsonb_array_elements / ->>'name'). (ord - 1) for 0-based, matching the
-- §B.1 tool_uses backfill in 20260505000001.
INSERT INTO claude_proxy.request_tools
    (request_id, request_created_at, ord, tool_name, created_at)
SELECT r.id, r.created_at, (ord - 1)::int, value, r.created_at
FROM claude_proxy.requests r
CROSS JOIN LATERAL
    jsonb_array_elements_text(r.tool_names) WITH ORDINALITY AS t(value, ord)
WHERE value IS NOT NULL
ON CONFLICT (request_id, request_created_at, ord) DO NOTHING;
```

### Migration B — `…/migrations/20260518000002_drop_requests_tool_names.sql` (destructive)

Ships in the same branch/PR but **after** the rehydrate read path no longer selects
`r.tool_names` and `migrations_smoke.rs` no longer asserts the GIN index.

```sql
-- Drop requests.tool_names + its GIN index; superseded by
-- claude_proxy.request_tools (20260518000001).
-- VIEW IMPACT AUDIT: no view/trigger references requests.tool_names
-- (project_fingerprint_view's tool aggregate reads tool_uses_v, verified).
-- GIN index idx_requests_tool_names_gin has ZERO query consumers (no
-- @>/?/?| anywhere in crates/claude-proxy/src; tool_names read/written
-- whole only).
DROP INDEX IF EXISTS claude_proxy.idx_requests_tool_names_gin;
ALTER TABLE claude_proxy.requests DROP COLUMN IF EXISTS tool_names;
```

No `SET LOCAL` timeout block — that scaffolding is unique to the single-cutover
`20260505000001`; the small siblings (`request_responses`, `request_diagnostics`) omit it.

## 2. Write path — `crates/claude-proxy/src/store/persistence/mod.rs`

In `insert_request_initial` (inside the existing `tx` from `self.pool.begin()`):

- Delete `let tool_names = serde_json::to_value(&req.tool_names)?;` (mod.rs:1098).
- Remove `tool_names` from the `INSERT INTO claude_proxy.requests` column list
  (mod.rs:1148) and its bind (mod.rs:1183). `tool_names` is positional `$11`;
  renumber `$12..$20` → `$11..$19`, re-count 19 binds vs 19 placeholders.
  **Highest-risk mechanical step** — gate on build + `crash_recovery` /
  `requests_check_constraints` round-trips.
- Add a `request_tools` INSERT loop after the `request_system_prompts` INSERT
  (after mod.rs:1265), **0-based `ord` via `enumerate()`** to match the backfill:

```rust
for (ord, tool_name) in req.tool_names.iter().enumerate() {
    sqlx::query(
        "INSERT INTO claude_proxy.request_tools
            (request_id, request_created_at, ord, tool_name)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (request_id, request_created_at, ord) DO NOTHING",
    )
    .bind(request_db_id).bind(req.created_at).bind(ord as i32).bind(tool_name)
    .execute(&mut *tx).await?;
}
```

## 3. Read path — `crates/claude-proxy/src/store/persistence/rehydrate.rs`

Four JSONB-agg sites: `fetch_request_by_id` (agg :454-465, `r.tool_names` :448);
`fetch_session_request_summaries_paginated` (:858-870); `list_requests_paginated`
filtered (:1155-1167) and unfiltered (:1195-1207).

- Add typed structs: `ToolUseRow { tool_id, tool_name, input: Option<Value> }`,
  `DeclaredToolRow { tool_name: String }`, and `impl From<ToolUseRow> for ToolCall`
  (`input_json` = `input.map(|v| v.to_string()).unwrap_or_default()`, `complete: true`
  with a one-line invariant comment: a persisted tool_use is post-terminal / fully
  captured).
- `fetch_request_by_id`: drop `r.tool_names` + the `jsonb_agg AS tool_calls` from SQL.
  After the row, run typed `SELECT … FROM tool_uses WHERE request_id=$1 AND
  request_created_at=$2 ORDER BY ord` and `SELECT tool_name FROM request_tools …
  ORDER BY ord`; pass `tool_calls: Vec<ToolCall>` + `tool_names: Vec<String>` into
  `row_to_intercepted` as params. Delete the `tool_names` decode (:225) and the
  `ToolCallDto` decode block (:228-235).
- 3 listing sites: **batch** to avoid N+1 — gather `(id, created_at)` keys, run one
  `request_tools` and one `tool_uses` (names+ord only) query via
  `UNNEST($1::bigint[], $2::timestamptz[])`; group into
  `HashMap<(i64,DateTime<Utc>), Vec<String>>`; pass slices into `row_to_request_summary`.
- `row_to_request_summary`: take `declared: Vec<String>` + `invoked_names: Vec<String>`
  params instead of decoding `row.tool_names` (:743) / `row.tool_calls` (:749). Delete
  `ToolCallNameOnly` (:620, used only here).
- Delete `ToolCallDto` (:75-91) + its `From`; the `serde_json::Value` row fields
  `PersistedRequestRow.tool_names`/`.tool_calls` (:133-134) and
  `RequestSummaryRow.tool_names`/`.tool_calls` (:665-666). **Keep `parse_json_column`
  (:196)** — still used for `params`/`token_usage`.
- `row_to_intercepted`: `tool_results: Vec::new()` (:330) / `tool_results_in_body:
  Vec::new()` (:336) **stay**. Rewrite the stale :322-329 comment to point at
  `GET /api/requests/{id}/tool-calls`.

## 4. New paired endpoint + MCP tool (API-layer only — no schema change)

- `crates/claude-proxy/src/api/handlers.rs`: new `PairedToolCall` DTO
  `{ tool_id, name, input: serde_json::Value, ord: i32, result: Option<ToolResultDto> }`
  (reuse existing `ToolResultDto` at handlers.rs:2215). New handler
  `get_request_tool_calls` → `{ request_id, total_count, calls: [...] }`. Emitter-scoped
  on path `{id}`. SQL: `tool_uses tu LEFT JOIN tool_results tr ON tr.tool_use_id =
  tu.tool_id WHERE tu.request_id = $1 ORDER BY tu.ord` (LEFT JOIN so an unanswered call
  is `result: null`, never a dropped row). `total_count` is a count *alongside* full
  rows (fidelity-permitted). `RequestDetail` unchanged.
- `crates/claude-proxy/src/store/persistence/mod.rs`: add
  `fetch_request_tool_calls_paired(pool, request_id)` alongside
  `fetch_tool_results_for_request` (mod.rs:2029); reuse `pub(crate) ToolUseRow`
  (mod.rs:144) / `ToolResultRow` (mod.rs:158).
- `crates/claude-proxy/src/api/manifest.rs`: add `RouteEntry { path:
  "/api/requests/{id}/tool-calls", coverage: Coverage::Tool("get_request_tool_calls") }`
  + the router route.
- `crates/proxy-mcp/src/tools.rs`: add the `get_request_tool_calls` MCP tool.
- Leave `get_request_tool_results` as-is (the parity tests + MCP catalogue depend on it).

## 5. Tests

- `crates/claude-proxy/src/store/operator_turns.rs` — fix the `#[cfg(test)]` seed
  (:271-287) that INSERTs `tool_names`.
- `crates/claude-proxy/tests/integration/migrations_smoke.rs` — remove the
  `("claude_proxy","idx_requests_tool_names_gin")` tuple (:72), add
  `idx_request_tools_name` + `idx_request_tools_request` + `claude_proxy.request_tools`,
  fix doc comment. Must land **with Migration B** in the same PR.
- All test files with a hardcoded `INTO claude_proxy.requests (... tool_names ...)`:
  authoritative list via
  `grep -rln "tool_names" crates/claude-proxy/tests/ | xargs grep -l "INTO claude_proxy.requests"`.
  Mechanical: drop `tool_names` column + `'[]'::jsonb` value.
- New tests (`TestDb::acquire`, per `.claude/rules/testing.md` — per-test ephemeral DB,
  no `#[ignore]`, no unscoped DELETE):
  1. Declared round-trip with duplicate `["Bash","Read","Bash"]` — ord 0/1/2, order kept,
     proves no `UNIQUE(tool_name)`.
  2. Empty declared → 0 rows, `fetch_request_by_id` yields `tool_names == []`.
  3. Pairing via the new endpoint: emitter with `toolu_A`, follow-up `tool_result` for
     `toolu_A` → `calls[0].result.tool_use_id == calls[0].tool_id`; unanswered call →
     `result: null`.
  4. Declared-vs-invoked non-conflation through
     `fetch_session_request_summaries_paginated`: `tool_names=["Read"]`,
     `tool_calls=[Bash,Bash]` → summary `tool_names==["Read"]`,
     `response_tools_used==["Bash"]`.
  5. Backfill: migration-level test of `jsonb_array_elements_text` + `ord = ordinality-1`
     against a synthetic string-array table.

## 6. Build sequence & verification

1. Migration A. 2. mod.rs write path → `cargo build -p claude-proxy`.
3. rehydrate.rs typed reads → `cargo build --all`. 4. New endpoint + manifest + MCP tool.
5. Migration B + `migrations_smoke.rs` tuple removal. 6. Fix breaking tests + seed.
7. New tests. Then, output captured in-terminal:

- `cargo build --all`
- `cargo clippy --all-targets -- -D warnings` (watch unused-import after `ToolCallDto`
  deletion; confirm `parse_json_column` still referenced)
- `cargo nextest run --all`
- `cargo nextest run -p proxy-mcp --test api_parity` (manifest parity green)
- **sqlx**: codebase uses runtime `sqlx::query_as`/`query` (not the compile-time
  `query!` macro) and `.sqlx/` holds only `.gitkeep` — verify with
  `grep -rl 'query!' crates/claude-proxy/src | head`; if empty, **no `cargo sqlx
  prepare` needed**. If any `query!` macro exists, run `cargo sqlx prepare --workspace`
  and commit `.sqlx/`.
- Inspection-fidelity self-check — paste full output, expect `CLEAN`. Pre-existing
  `_summary` identifiers (`RequestSummary`, `row_to_request_summary`) are justified
  full-row projections (rule "list of full rows (OK)"):

```bash
git diff --stat | head -30 \
  && echo "--- forbidden patterns ---" \
  && git diff -- '*.svelte' '*.ts' '*.tsx' '*.rs' '*.sql' \
     | grep -nE '(\b(line-clamp|truncate|text-ellipsis)\b|\.slice\(0,|\.substring\(0,|\.substr\(0,|…|\[truncated\]|\b(head_n|clip)\(|_preview\b|_summary\b)' \
     || echo CLEAN
```

## 7. File allowlist

**Created**
- `crates/claude-proxy/src/store/persistence/migrations/20260518000001_request_tools_table.sql`
- `crates/claude-proxy/src/store/persistence/migrations/20260518000002_drop_requests_tool_names.sql`
- `crates/claude-proxy/tests/e2e/request_tools_roundtrip.rs` + a `[[test]]` entry in
  `crates/claude-proxy/Cargo.toml`

**Modified (production)**
- `crates/claude-proxy/src/store/persistence/mod.rs`
- `crates/claude-proxy/src/store/persistence/rehydrate.rs`
- `crates/claude-proxy/src/api/handlers.rs`
- `crates/claude-proxy/src/api/manifest.rs`
- `crates/proxy-mcp/src/tools.rs`
- `crates/claude-proxy/src/store/operator_turns.rs` (`#[cfg(test)]` seed only)

**Modified (tests)**
- `crates/claude-proxy/tests/integration/migrations_smoke.rs`
- `crates/proxy-mcp/tests/api_parity.rs` (only if it enumerates tools/routes explicitly)
- The `tool_names`-INSERT test files enumerated by the §5 grep (mechanical column drop)

**Explicitly NOT modified**
- `RequestDetail`/`RequestSummary` DTO shapes, the in-memory
  `From<&InterceptedRequest>` path (handlers.rs:2843),
  `extract::request::extract_tool_names`, `proxy/handler.rs` (still populates
  `req.tool_names`), `parse_json_column`, `get_request_tool_results`,
  `project_fingerprint_view` (reads `tool_uses_v`, not `requests.tool_names`).

## Risks

1. **`$11` placeholder renumber** in the `requests` INSERT — only error-prone step;
   gated by build + crash-recovery/check-constraints round-trips.
2. **Migration B ordering** — must run after the rehydrate code stops selecting
   `r.tool_names` and after the `migrations_smoke.rs:72` tuple is removed.
3. **Test blast radius** (~20 files) — wide but uniform (drop one column+value).
4. **Declared-vs-invoked conflation** — guarded by the preserved in-memory unit test +
   new DB test #4.
5. **Backfill element-type bug** (the original `->>'name'` would NULL every row) —
   fixed via `jsonb_array_elements_text` + `WHERE value IS NOT NULL`; covered by test #5.
