# temet-proxy Architecture

This file is the **developer quick-start**: the invariants, conventions, and mechanical rules you need before touching any code. Read it before your first PR. It is fully self-contained — an agent reading only this file can determine where new code goes and how to add any new feature end-to-end.

For full historical rationale, decision log, and deep subsystem specs, see [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

---

## Table of Contents

**Rust workspace**

1. [Workspace layout](#1-workspace-layout)
2. [Axum handler structure](#2-axum-handler-structure)
3. [MCP crate (mike-mcp)](#3-mcp-crate-mike-mcp)
4. [The `test-seams` feature flag](#4-the-test-seams-feature-flag)
5. [Clippy enforcement](#5-clippy-enforcement)
6. [Error handling](#6-error-handling)
7. [Configuration](#7-configuration)
8. [Four-tier test taxonomy](#8-four-tier-test-taxonomy)
9. [Database](#9-database)
10. [Script conventions](#10-script-conventions)
11. [How to add a new backend feature end-to-end](#11-how-to-add-a-new-backend-feature-end-to-end)
12. [Database migration walkthrough](#12-database-migration-walkthrough)

**Frontend**

13. [Storybook-first workflow](#13-storybook-first-workflow)
14. [Design corpus](#14-design-corpus)
15. [Component hierarchy](#15-component-hierarchy)
16. [Runes-backed stores](#16-runes-backed-stores)
17. [Storybook-stubs](#17-storybook-stubs)
18. [Vitest two-project setup](#18-vitest-two-project-setup)
19. [a11y gate](#19-a11y-gate)
20. [TypeScript rules](#20-typescript-rules)
21. [How to add a new component](#21-how-to-add-a-new-component)

---

## RUST WORKSPACE

### 1. Workspace layout

```
crates/
├── mike/           — The proxy + inspect bind binary.
│   src/            Two Axum servers (proxy :9988 + inspect :9989)
│                   sharing one AppState. Owns the capture pipeline,
│                   SSE tee, filter, composer, weasel intercept, DB
│                   writes, WebSocket broadcast, and all REST routes.
│
└── mike-mcp/       — Separate MCP bridge binary.
    src/            Spawned by Claude Code over stdio. Talks HTTP to
                    the inspect bind (:9989). Exposes 52 tools + 3
                    resources via rmcp 0.17 (run `mike-mcp list-tools`
                    for the authoritative count). NEVER imports
                    mike as a library crate.
```

**One process, two servers.** `mike` runs two `axum::serve` futures sharing one `AppState`:

| Bind | Port | Routes | Contract |
|---|---|---|---|
| Proxy | 9988 | `/v1/*` → `api.anthropic.com` | Injecting/optionally-mutating pipeline (filter rules, lesson/reminder/steerer injection, composer splicing, weasel intercept); identity is the no-op default when no rule matches — capture always stores the untouched inbound bytes |
| Inspect | 9989 | `/api/*` REST + `/api/ws` WS | CORS-locked, normal middleware |

**Three-process deployment.** The SPA runs as an independent `vite dev` process on `:9990`. The MCP bridge is a separate binary (`mike-mcp`) spawned by Claude Code. Neither is embedded in the Rust binary.

**Workspace-level dependencies** are pinned in the root `Cargo.toml` `[workspace.dependencies]` — add a dep there first, then refer to it with `{ workspace = true }` in the crate manifest. This prevents version drift between crates.

#### Module map (`crates/mike/src/`)

| Module | Purpose |
|---|---|
| `proxy/` | Core proxy domain. `handler.rs` is the axum handler for `/v1/*`. `upstream.rs` handles reqwest forwarding and URI rewriting. `headers.rs` is pure header filtering/rewriting. |
| `extract/` | Pure functions for request/response body inspection. No async, no state, fully unit-testable. |
| `sse/` | SSE parsing and tee. `tee.rs` is the channel-based byte-tee. `parser.rs` emits `ProxyEvent`s. |
| `store/` | In-memory request store. `persistence/` holds Postgres write paths. `operator_turns.rs` manages the OPEN→CLOSED lifecycle. |
| `api/` | Inspect bind REST/WS. `mod.rs` builds the axum router. `handlers/` holds per-resource REST handlers. `manifest.rs` enforces MCP parity at build time. |
| `filter/` | System-prompt filter pipeline. Drop/replace/append rules against `system[]` and `messages[*].content[]`. `FilterConfig` is hot-swapped via `ArcSwap`. |
| `compose_prompt/` | Prompt composition pipeline. Runs after filter. Byte-splice (not parse-and-reserialize) to preserve cache keys. |
| `reminders/` | Appends `<system-reminder>` blocks from prior analyzer findings to outgoing requests. |
| `weasel_intercept/` | Buffer-then-decide SSE intercept. On `WeaselSignal::Hit`, discards buffer and issues a correction request. |
| `agents/` | Agent registry domain. `AgentSource` enum (Observed, FileBacked, Dynamic). |
| `analysis/` | Diagnostic pattern analysis. 19 patterns across 5 categories; reporting only, never injected. |
| `config/` | `ProxyConfig` + figment 4-layer pipeline. `AppState` construction. `paths.rs` is the XDG lookup chain. |
| `diagnostics/` | Pluggable analyzers registered in `AnalyzerRegistry` (hot-swapped via `ArcSwap`). |
| `observability/` | Structured tracing setup, log format selection. |
| `memory_cap/` | Global RSS cap manager. `Sheddable` trait + `ShedRegistry`. Background RSS tick loop. |
| `in_flight.rs` | In-flight request registry. DashMap-backed with RAII `InFlightGuard`. Hard cap returns HTTP 503. |
| `sweep.rs` | DB-side TTL sweep. Single SQL DELETE per tick. |
| `db_listener.rs` | Postgres LISTEN/NOTIFY on `proxy.row_changed`. Re-broadcasts as `ProxyEvent::RowChanged`. |
| `startup.rs` | Hot-reload config watcher. Boot-time stale-row recovery. |
| `main.rs` | CLI parse, tracing init, server start (~50 lines). |

---

### 2. Axum handler structure

#### Thin handlers, no Tower middleware on the body path

The proxy handler (`proxy/handler.rs`) is **not** Tower middleware. Tower is used only for non-body cross-cutting concerns: `TraceLayer`, `DefaultBodyLimit`. The handler owns the full request pipeline in sequence:

```
extract_request_parts()
  → filter pipeline        (drop/replace/append text blocks)
  → reminders pipeline     (append <system-reminder> findings)
  → compose_prompt pipeline (byte-splice system[2] for first-of-session)
  → build InterceptedRequest
  → store.insert(req)
  → insert_request_initial   ← Phase 1 DB write (stub row)
  → in_flight.register()     → HTTP 503 if at capacity
  → client.post(upstream).send()
  → tee SSE stream           (or buffer non-streaming)
  → snapshot_request()       ← Phase 2 DB write (terminal UPDATE)
  → NOTIFY proxy.row_changed
```

The inspect bind (`api/mod.rs`) builds a separate axum `Router`. Every handler function is a thin dispatcher: read from `AppState`, call a service/persistence function, return a typed response.

#### Adding a new inspect route

Three coordinated edits are required — any missing fails a test:

1. **Add the route** in `api/mod.rs` (or the relevant handler file under `api/handlers/`).
2. **Add a `manifest.rs` entry** in `api/manifest.rs` with its `Coverage` variant:
   - `Coverage::Tool("tool_name")` — wraps a named MCP tool 1:1.
   - `Coverage::AggregatedInto("tool_name")` — data folded into an aggregate tool.
   - `Coverage::Resource("proxy://uri")` — surfaced as an MCP resource.
   - `Coverage::Exempt { reason: "..." }` — deliberately not exposed via MCP.
3. **Add the MCP tool** in `crates/mike-mcp/src/tools.rs` (or declare the exemption).

The parity test at `crates/mike-mcp/tests/manifest_coverage.rs` walks both directions and fails the build if either side is missing.

---

### 3. MCP crate (mike-mcp)

**The MCP bridge is a thin HTTP client, not a shared library.** It has no `Cargo.toml` dependency on `mike`. It talks HTTP to the inspect bind on `:9989`. Tool implementations in `tools.rs` look like:

```rust
// Thin: parse input → HTTP call → return result
pub async fn handle_list_requests(client: &InspectClient, args: Value) -> CallToolResult {
    let offset = args["offset"].as_u64().unwrap_or(0);
    let limit  = args["limit"].as_u64().unwrap_or(20);
    client.get_json(&format!("/api/requests?offset={offset}&limit={limit}")).await
}
```

**No business logic lives here.** If a tool does more than deserialize arguments, call one HTTP endpoint, and return the result, the logic belongs in the inspect bind, not the MCP bridge.

#### Adding an MCP tool

1. Add a `pub const TOOL_<NAME>: &str = "tool_name";` constant at the top of `tools.rs`.
2. Add a `Tool` registration in `server.rs::registered_tools()` with the input schema.
3. Add a handler branch in the `call_tool` dispatcher.
4. Add the corresponding `Coverage::Tool("tool_name")` entry in `mike/src/api/manifest.rs`.
5. Run `cargo nextest run -p mike-mcp` — the parity test will now pass.

**All tracing goes to stderr.** Stdout is the JSON-RPC wire — a stray log line corrupts the MCP protocol.

---

### 4. The `test-seams` feature flag

Some integration tests need to inject a wiremock server URL as the upstream, bypassing the real `api.anthropic.com`. The `test-seams` feature on the `mike` crate enables compile-time seams for this substitution.

**The self-dep trick:** `mike` lists itself in `dev-dependencies` with `features = ["test-seams"]`. This means the integration test binaries (compiled from `tests/`) see the crate compiled with the test-seams feature, while the library and the `mike` binary itself are compiled without it. Production code is never affected.

To use a seam in a test:

```rust
// In tests/common/mod.rs
#[cfg(feature = "test-seams")]
pub fn build_app_state_with_mock(upstream_url: &str) -> AppState { ... }
```

Never use `#[cfg(test)]` in `src/` files to gate production behaviour — use the feature flag instead.

---

### 5. Clippy enforcement

The workspace toolchain config applies to every crate in the workspace:

```toml
[target.'cfg(all())']
rustflags = [
    "-D", "clippy::unwrap_used",
    "-D", "clippy::expect_used",
    "-D", "clippy::complexity",
    "-D", "clippy::cognitive_complexity",
    "-D", "clippy::too_many_arguments",
    "-D", "clippy::too_many_lines",
]
```

The `unwrap_used` and `expect_used` lints are **errors**, not warnings. Every
`unwrap()` or `expect()` in `src/` is a build failure.

**In `src/` (application crates)**: use `?` to propagate errors. Use exhaustive `match` on `Result` and `Option`. If you need to assert something is unreachable, use `unreachable!()` (panics are acceptable for truly unreachable states; `unwrap` is not because it hides intent).

**In `tests/` and `main.rs`**: `expect()` with a descriptive message is permitted — test failures and startup failures are acceptable panics.

**`unsafe_code` is denied** by `#![deny(unsafe_code)]` in every `src/lib.rs`. No exceptions without explicit team discussion.

**CI command:** `scripts/lint-rust`

The structural limits are mandatory across every target:

- cognitive complexity: **15** maximum per function;
- function length: **100** lines maximum;
- function arguments: **6** maximum;
- Rust source file length: **500** lines maximum.

The measurable structural lints and broader Clippy complexity group are denied.
`scripts/lint-rust-suppressions` rejects repository attributes that could lower
those lint levels; this avoids breaking broad lint attributes emitted internally
by third-party derive macros while still preventing source-level escape hatches.
`scripts/lint-rust-size` provides the file-length check that Clippy does not
implement. There is no legacy allowlist: existing violations are real gate
failures and must be split by responsibility.

`scripts/lint-rust-architecture` enforces two dependency directions without a
build. Code under `store/` cannot import or otherwise name `crate::api`: storage
returns domain or read-model types, and the HTTP adapter maps them to wire DTOs.
Axum types are confined to `api/`, `proxy/`, and the explicitly listed HTTP
composition/extraction adapters (`app.rs`, `main.rs`, `spa.rs`, `error.rs`, and
the two inbound request extractors). Comments and string literals are ignored;
there is no attribute, inline waiver, or legacy baseline that exempts source.

---

### 6. Error handling

Three layers, each with a different crate:

| Layer | Crate | Where |
|---|---|---|
| Domain errors | `thiserror` | Library code in `src/`. Named error enums (`ProxyError`, `SseError`, `McpError`). |
| HTTP mapping | custom `IntoResponse` | Handler layer maps domain errors to HTTP status codes. |
| Startup / CLI | `anyhow` | `main.rs`, `subcommands.rs`, and tests only. |

**Library code never uses `anyhow`.** If you're in `src/` writing a function that returns `Result`, the error type must be a named type derived with `thiserror`. `anyhow::Result` is only for binaries and tests.

**Exhaustive `match` on errors.** When a function returns a typed error, callers must match every variant. A wildcard `_ => ...` arm for a domain error is a smell — add a new variant to the enum instead.

**Upstream error passthrough.** When the upstream returns 4xx/5xx, the proxy forwards the response verbatim. Do not wrap upstream errors in `ProxyError` — the byte-identity contract applies to error responses too.

---

### 7. Configuration

Config loads in a 3-layer `figment` pipeline. Later layers win:

```
defaults  →  MIKE_* env vars  →  CLI flags
```

There is no runtime file-backed config lookup anymore. The proxy boots from built-in defaults, process env, and CLI flags only. The live operator source of truth is `/settings`.

**Operator CLI surface:** Two subcommands remain for managing boot config out-of-process:
- `mike print-config [--shell]` — resolved effective values
- `mike migrate-check` — validate the current database/config state before startup

**`scripts/start-proxy.sh` sources the binary:**

```bash
eval "$(mike print-config --shell)"
```

The shell script has no config knowledge of its own. If you add a config key, add it to the `print-config --shell` output — the script picks it up automatically.

**Key sub-tables:** `[proxy]`, `[inspect]`, `[weasel_intercept]`, `[reminders]`, `[persistence]`, `[lesson_worker]`, `[steerer]`, `[cap_manager]`. The runtime settings catalog is edited in `/settings`. Filter rules are **not** a config surface — they are DB-backed and authored at runtime via the inspect API / MCP `*_filter_rule` tools.

---

### 8. Four-tier test taxonomy

Tests are split across four named `[[test]]` binary targets so nextest can run them in parallel:

| Tier | Named target | What it covers | DB? | Upstream? |
|---|---|---|---|---|
| **Proxy** | `proxy_tests` | Handler pipeline, SSE tee, filter, weasel intercept, config | No | Wiremock (no real Anthropic) |
| **Integration** | `integration_tests` | DB writes, persistence, migration round-trips, operator turn lifecycle | Real Postgres | No |
| **Schema** | `schema_tests` | Migration checksums — fails if a migration file changes after merging | Real Postgres | No |
| **E2E** | `e2e_tests` | Full system: proxy bind + inspect bind + DB + real SSE stream | Real Postgres | Wiremock |

**nextest runs all four in parallel** across separate processes. Cross-binary env races are impossible because each binary is a separate OS process with its own env.

**`serial_test` for env-mutating tests.** Tests that mutate process-global env (`MIKE_CONFIG`, `HOME`, `XDG_CONFIG_HOME`) within a single binary use `#[serial_test::serial(<group>)]`. This gates those tests within their binary without serialising the whole workspace.

**NEVER `RUST_TEST_THREADS=1`.** This was removed because it serialised the entire workspace — 30-minute test runs. Per-binary `#[serial]` is the correct fix. The workspace toolchain config has a comment explaining this.

**`make test`** runs `cargo nextest run --all`. `make test-p p=<crate>` scopes to one crate. `make test-t t=<filter>` runs a single test by name pattern.

---

### 9. Database

**sqlx with compile-time query checks.** All queries use `sqlx::query!` or `sqlx::query_as!` macros which verify SQL at compile time against the target schema. Normal builds use the committed `.sqlx` offline cache — no DB connection and no `DATABASE_URL`. After adding/changing a query, refresh the cache with `cargo sqlx prepare --database-url "$MIKE_DATABASE_URL"` (the repo's only DB var).

**Migrations on startup.** `startup.rs` calls `sqlx::migrate!("./migrations").run(&pool)` before the servers bind. The proxy never runs against an unmigrated schema.

**pgvector test environment.** DB-backed tests provision ephemeral `claude_proxy_test_<uuid>` databases via `MIKE_TEST_ADMIN_URL` (a reachable admin Postgres, set in the workspace toolchain config with `force = false` so CI can override). There is no local Docker DB — the admin URL points at a remote/managed Postgres.

**Test DB isolation.** Each integration test that writes to the DB must:
1. Use a unique schema or truncate tables in its fixture setup.
2. Use the `test_support.rs` helpers (`setup_test_db`, `teardown_test_db`) which run migrations against a fresh schema.
3. Mark with `#[serial_test::serial(db)]` if it touches shared tables.

**Schema tests (`schema_tests` binary).** After a migration is merged, its checksum is fixed. Schema tests compute the SHA-256 of every migration file in `store/persistence/migrations/` and compare against stored expected values. This catches the "I edited a migration that's already applied in prod" class of mistake early.

**Two-phase write.** Every captured request goes through two DB writes:
1. `insert_request_initial` — stub `Pending` row before upstream send. Enables FK-valid `response_stream_chunks` inserts during streaming.
2. `snapshot_request` — terminal UPDATE + `INSERT INTO request_responses` at completion.

Rows stuck as `Pending` after a crash are healed by `recover_stale_pending_rows()` on the next startup.

---

### 10. Script conventions

The `scripts/` directory contains the full operational toolchain. All scripts are idempotent and safe to re-run.

#### Port table

| Port | Service | Purpose |
|---|---|---|
| 9988 | Proxy bind | `/v1/*` — set `ANTHROPIC_BASE_URL=http://127.0.0.1:9988` |
| 9989 | Inspect bind | `/api/*` REST + `/api/ws` WebSocket + CORS |
| 9990 | SPA host | SvelteKit SPA (`vite dev` with HMR, or `pnpm run preview`) |
| 6006 | Storybook | Component story sandbox |

#### `scripts/start-proxy.sh`

Builds and deploys the full stack, without asking: builds the Rust binaries, hands the proxy to `deploy-proxy.sh` (smoke test on alternate ports, then cutover; a byte-identical proxy is left running), resolves config via `eval "$(mike print-config --shell)"`, and makes sure the sidecar, the prompt assistant, the web UI (Vite on 9990), Storybook (6006) and the architecture diagrams (9986) are up. Each of those is a systemd user unit (`mike-stack-*`), so the script returns the terminal when it is done. It ends with a box listing every address, in which a service that does not answer is marked. One deploy runs at a time: a second waits on `.deploy/deploy.lock`.

```bash
./scripts/start-proxy.sh           # optimised (release) build, the default
./scripts/start-proxy.sh --debug   # unoptimised build: faster to compile, slower to run
```

The script has no config knowledge of its own — it reads all port and path values from the Rust binary via `print-config --shell`. If you add a config key and want the script to pick it up, add it to the `print-config --shell` output.

#### `scripts/stop-proxy.sh`

Stops the `mike-stack-*` units, so systemd does not bring them back, then frees the stack's ports as belt-and-suspenders cleanup for anything running outside a unit. Works even when the binary is not on PATH — falls back to port defaults.

```bash
./scripts/stop-proxy.sh
MIKE_STORYBOOK_PORT=6007 ./scripts/stop-proxy.sh   # custom Storybook port
```

#### `scripts/install.sh`

Interactive one-shot setup wizard. Walks through: prerequisite checks (Rust, Node), Rust workspace build, frontend dep install, storage mode choice (in-memory / external Postgres), surgical `.env` bootstrap update (touches only the five managed keys, shows diff, asks before writing), and MCP bridge snippet for `~/.claude.json`. Safe to re-run.

```bash
./scripts/install.sh
./scripts/install.sh --non-interactive in-memory
./scripts/install.sh --non-interactive external  # requires MIKE_DATABASE_URL
```

#### PID tracking pattern

Where there is no systemd (`MIKE_STACK_SUPERVISOR=nohup`, macOS), `start-proxy.sh` runs the web UI, Storybook and the diagrams in the foreground and uses a PID tracking pattern: each background process's PID is captured and stored. The EXIT trap iterates them, sends SIGTERM, and waits for each. This ensures clean teardown even on Ctrl-C mid-startup. Under systemd nothing runs in the foreground and the trap has nothing to stop.

---

### 11. How to add a new backend feature end-to-end

This walkthrough covers the full path from database to MCP tool. Follow every step in order — partial implementations will fail the parity test.

**Step 1 — Write the sqlx migration.**

Create a new file in `crates/mike/src/store/persistence/migrations/`:

```
YYYYMMDDNNNNNN_description.sql
```

For example: `20260601000001_my_feature_table.sql`. The name must be unique, lexicographically ordered after all existing migrations, and the timestamp prefix must be the actual date. Write idempotent SQL — use `IF NOT EXISTS`, `ON CONFLICT DO NOTHING`, or `CREATE OR REPLACE` where possible. `sqlx::migrate!` runs all pending migrations on startup in filename order.

**Step 2 — Add a repository query.**

Add the query to the appropriate module under `crates/mike/src/store/persistence/`. Use `sqlx::query!` or `sqlx::query_as!` macros — never raw string SQL at call sites. Queries compile against the committed `.sqlx` offline cache; after adding/changing one, refresh it with `cargo sqlx prepare --database-url "$MIKE_DATABASE_URL"` (the repo's only DB var — there is no `DATABASE_URL`).

```rust
// In store/persistence/my_feature.rs
pub async fn fetch_my_feature(pool: &PgPool, id: i64) -> Result<MyFeatureRow, ProxyError> {
    sqlx::query_as!(
        MyFeatureRow,
        "SELECT id, name, created_at FROM mike.my_feature WHERE id = $1",
        id
    )
    .fetch_one(pool)
    .await
    .map_err(ProxyError::from)
}
```

**Step 3 — Add an inspect API route handler.**

Add the route handler in `crates/mike/src/api/handlers/`. Keep it thin: validate input, call the service/persistence function, return a typed JSON response.

```rust
// In api/handlers/my_feature.rs
pub async fn get_my_feature(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<MyFeatureRow>, ProxyError> {
    let persist = state.persist.as_ref().ok_or(ProxyError::PersistenceDisabled)?;
    let row = fetch_my_feature(&persist.pool, id).await?;
    Ok(Json(row))
}
```

Register the route in `api/mod.rs`:

```rust
.route("/api/my-feature/:id", get(handlers::my_feature::get_my_feature))
```

**Step 4 — Add a `manifest.rs` entry.**

Every route must declare its MCP coverage in `crates/mike/src/api/manifest.rs`. Choose the right variant:

```rust
RouteEntry {
    method: "GET",
    path: "/api/my-feature/:id",
    coverage: Coverage::Tool("get_my_feature"),
},
```

The parity test reads this manifest. Adding the route without a manifest entry fails the build.

**Step 5 — Add the MCP tool in `mike-mcp`.**

In `crates/mike-mcp/src/tools.rs`:

1. Add a constant: `pub const TOOL_GET_MY_FEATURE: &str = "get_my_feature";`
2. Add a `Tool` registration in `server.rs::registered_tools()` with name, description, and input schema.
3. Add a handler branch in `call_tool`:

```rust
TOOL_GET_MY_FEATURE => {
    let id = args["id"].as_i64().ok_or(McpError::InvalidArgs("id required"))?;
    client.get_json(&format!("/api/my-feature/{id}")).await
}
```

**Step 6 — Verify parity test passes.**

```bash
cargo nextest run -p mike-mcp --test manifest_coverage
```

If the test fails, re-check that the manifest entry tool name exactly matches the `TOOL_*` constant string.

**Step 7 — Add integration tests.**

Add tests to the `integration_tests` binary target in `crates/mike/tests/`. Use `setup_test_db` + `teardown_test_db` helpers. Mark with `#[serial_test::serial(db)]` if the test touches shared tables. Run with:

```bash
make test-p p=mike
```

---

### 12. Database migration walkthrough

#### Creating a migration

1. Choose the next filename using today's date: `YYYYMMDD000001_description.sql`. If another migration for today already exists, increment the sequence: `YYYYMMDD000002_`.
2. Write idempotent SQL. Use `CREATE TABLE IF NOT EXISTS`, `ADD COLUMN IF NOT EXISTS`, `CREATE INDEX IF NOT EXISTS`, etc. Migrations are run against production databases; they must survive being run twice without error.
3. Scope to the `mike` schema: `CREATE TABLE mike.my_table (...)`.
4. Prefer additive changes. Never DROP a column in the same migration that removes its usage — separate those into different deploys.

#### Migration naming convention

```
YYYYMMDDNNNNNN_description.sql
```

- `YYYYMMDD` — date (UTC)
- `NNNNNN` — 6-digit sequence within the day, zero-padded (`000001`, `000002`, ...)
- `description` — snake_case, short (e.g., `add_embedded_at_to_operator_turns`, `normalised_tool_data_plane`, `bodies_to_postgres`)

The lexicographic order of filenames IS the migration execution order. Never rename a migration file after it has been merged to main — schema tests checksum every file and fail on any change.

#### Running `cargo sqlx prepare`

`sqlx::query!` macros verify SQL at compile time. When you add or change a query, you must regenerate the offline query cache so CI can build without a live DB:

```bash
# The repo's only DB var is MIKE_DATABASE_URL; pass it to sqlx via --database-url
# (no DATABASE_URL env var is used or set):
cargo sqlx prepare --workspace --database-url "$MIKE_DATABASE_URL"
```

This writes `sqlx-data.json` files. Commit them with the migration and the code that uses it. CI runs `cargo sqlx prepare --check` to verify the cache is in sync.

#### Testing with the schema test tier

After writing a migration:

1. Run it against your local test DB: `cargo nextest run --test schema_tests`
2. If you see a checksum failure, you edited a migration that was already applied. Fix the issue in a NEW migration file, never by editing the existing one.
3. The schema test tier runs automatically in CI via `make test`.

#### When the proxy is running against an existing DB

Migrations run automatically on the next proxy startup via `sqlx::migrate!("./migrations").run(&pool)` in `startup.rs`. For a large migration (e.g. backfilling a new column across millions of rows), either:
- Write the migration so it is fast (add the column nullable first, backfill as a separate step via a background task, add the NOT NULL constraint later), or
- Stop the proxy, run the migration manually with `psql`, restart.

---

## FRONTEND

### 13. Storybook-first workflow

**Stories are the living spec. Write them before the component exists.**

The mandatory sequence for every new UI element:

```
1. Sketch the component API — props only, empty body is fine.
2. Write Component.stories.svelte covering every state.
3. Storybook (already running as `mike-stack-storybook`; else `pnpm run storybook`) — iterate on visuals.
4. Get design feedback (screenshot or Storybook URL).
5. Implement the component body so all stories render correctly.
6. Wire into a route or parent component.
```

**Why this order matters:**

- Writing stories first forces enumeration of all states (empty, loading, error, disabled, dark) before a single line of implementation logic.
- It decouples the component from app context — a component that can't be storyed has hidden coupling.
- Design feedback happens on real rendered HTML, not Figma frames.
- The Vitest + Storybook addon turns every story into a CI test automatically — no parallel test setup needed.

Stories live **colocated** with the component:

```
src/lib/components/
├── StatusPill.svelte
├── StatusPill.stories.svelte    ← Svelte CSF via @storybook/addon-svelte-csf
└── StatusPill.test.ts           ← unit tests for any non-trivial logic
```

**Storybook runs on `:6006`** as the systemd user unit `mike-stack-storybook` (`pnpm run storybook` where there is no systemd). `scripts/start-proxy.sh` lists this alongside the other ports in its usage block.

---

### 14. Design corpus

#### Tailwind v4 + `@theme inline`

All design tokens live in `src/app.css`. The layering is:

```
:root { ... }         — primitive values (hex colours, rem sizes)
@theme inline { ... } — Tailwind utility exposure
```

The `@theme inline` directive tells Tailwind to treat values as CSS-variable references at runtime, not static values baked at build time. Without `inline`, dark-mode overrides would break because Tailwind inlines the light-mode value.

**This app is dark-only.** The surface is `#0f1117` (`--bg`). There is no light mode.

**CI enforcement:** `grep -rn "\bdark:" frontend/src` **outside** `components/ui/` must return zero matches. Application components read `var(--*)` tokens; they never use `dark:` prefixes. The `@custom-variant dark` directive exists solely for `shadcn-svelte` primitives under `ui/` — those are the single exception.

#### Adding a new design token

1. Add `:root { --my-token: ...; }` declaration in `app.css`.
2. Add `@theme inline { --color-my-token: var(--my-token); }` if you want a Tailwind utility (`bg-my-token`).
3. Never reach for an ad-hoc hex value in a component file. If a token doesn't exist, add it here.

#### Available token categories

| Category | Examples |
|---|---|
| Surfaces | `--bg`, `--surface`, `--surface-2`, `--sidebar` |
| Borders | `--border-color`, `--border-subtle` |
| Text | `--text-strong`, `--text-1`, `--text-2`, `--text-3`, `--text-faint` |
| Accents | `--green`, `--amber`, `--blue`, `--red`, `--destructive` |
| Structural | `--radius`, font stacks |

Full token list: `src/app.css`.

---

### 15. Component hierarchy

Three tiers, each with a strict rule about what it may import:

```
ui/             ← shadcn-svelte primitives — you own this code (copied, not installed as a dependency).
                  Do not modify unless you're intentionally diverging from shadcn.
                  May use bits-ui directly.
    ↓ composes
feature         ← Domain components built from ui/ primitives.
                  NEVER import bits-ui directly.
                  Live flat in src/lib/components/ or in subdirectories by domain.
    ↓ used in
routes/         ← Page files (+page.svelte) import feature components only.
                  No component logic inside a page file.
```

**`bits-ui` isolation:** Feature components never reach into `bits-ui`. If you find yourself importing from `bits-ui` in a feature component, you're either missing a `ui/` primitive (add it) or misusing the abstraction.

**The `...restProps` contract.** Every `ui/` primitive spreads `...restProps` onto its underlying element. This is load-bearing — consumers rely on it to pass `aria-label`, `id`, `data-*`, `onclick`, etc. without the component needing to enumerate every DOM attribute. Feature components do not need this (they have defined props contracts).

**`data-slot` for testing/styling.** Every visual element in a `ui/` component has a `data-slot="<name>"` attribute. This is the stable hook for consumers to target elements in CSS and tests without coupling to class names.

---

### 16. Runes-backed stores

All shared reactive state lives in `src/lib/stores/*.svelte.ts`. The `.svelte.ts` extension is **required** — runes (`$state`, `$derived`) are only available inside `.svelte.ts` and `.svelte` files.

#### Naming and ownership convention

- **Factory function pattern**: every store is created by a `createXxxStore()` function, not exported as a module-level singleton.
- **`setContext` ownership**: the root layout (`routes/+layout.svelte`) is the sole owner. It calls the factory once on mount and publishes the instance via `setContext(KEY, store)`.
- **`getContext` consumption**: child routes call `getContext(KEY)` to access the store. They **must not** instantiate a second store — WS overlay state and throttle timers are per-instance and would diverge.
- **Context keys** live in `src/lib/context-keys.ts` as `Symbol()` values. Each store re-exports its key from that file for convenient co-import.

#### Adding a new store

1. Create `src/lib/stores/my-thing.svelte.ts`.
2. Add a `Symbol()` key to `context-keys.ts` and re-export it from the new store file.
3. Call the factory in `routes/+layout.svelte` and `setContext(MY_THING_KEY, store)`.
4. Consume with `getContext(MY_THING_KEY)` in child routes.
5. Add a stub under `__storybook-stubs__/` (see §17).

#### Trailing-edge throttle pattern

Stores that refetch on WebSocket deltas use a trailing-edge throttle to avoid hammering the inspect bind on bursty events:

```typescript
const THROTTLE_MS = 500;

function scheduleRefresh() {
    clearTimeout(throttleTimer);
    throttleTimer = setTimeout(() => refresh(), THROTTLE_MS);
}
```

`Lagged` WebSocket frames bypass the throttle and trigger an immediate `refresh()` — a lagged frame means the client is behind and must catch up now.

---

### 17. Storybook-stubs

Stories run in an isolated Storybook canvas without access to singletons like the WebSocket connection, auth state, or the real API client. The `__storybook-stubs__/` directory provides per-module replacements wired at build time.

#### How it works

The Storybook Vite config aliases module paths so that when a story imports (say) `$lib/ws.svelte.ts`, it gets `$lib/__storybook-stubs__/ws.svelte.ts` instead. The alias is declared in `.storybook/main.ts`'s `viteFinal` hook.

**Alias ordering matters.** More-specific paths must come before prefix matches:

```typescript
// Correct — longer match first
{ find: '$lib/stores/sessions.svelte.ts',   replacement: stubsDir + '/stores-sessions.svelte.ts' },
{ find: '$lib/stores',                       replacement: realStoresDir },
```

#### Adding a stub for a new singleton

1. Create `src/lib/__storybook-stubs__/my-singleton.svelte.ts` exporting the same interface as the real module but with static/no-op implementations.
2. Add the alias in `.storybook/main.ts`:
   ```typescript
   { find: '$lib/my-singleton.svelte', replacement: stubsDir + '/my-singleton.svelte.ts' }
   ```
3. Verify in Storybook that stories using the stubbed module render without errors.

**Stubs are for isolation only.** They must not contain production logic — any shared logic belongs in the real module, not the stub.

---

### 18. Vitest two-project setup

`vitest.config.ts` defines two named test projects:

| Project | Environment | What it tests | File pattern |
|---|---|---|---|
| `unit` | node | Pure TS functions, store reducers, formatters, validators | `src/**/*.test.ts` (excludes `.svelte.test.ts`) |
| `storybook` | browser/chromium (Playwright) | Stories rendered in real Chromium: smoke, interaction, a11y | Stories discovered by `storybookTest` plugin |

**`vitest.config.ts` is separate from `vite.config.ts`.** The SvelteKit Vite config (`vite.config.ts`) is for the app build. The Vitest config is for tests only. Mixing them causes the `storybookTest` plugin to interfere with the build.

**Which project for new tests:**

- Pure function → `unit` project. Filename `*.test.ts`.
- Component rendering / interaction / a11y → `storybook` project. Write a story (see §13), not a separate test file.
- Full multi-route user flow → Playwright (not covered by this config; run separately).

**Running tests:**

```bash
pnpm test                    # both projects
pnpm run test:unit           # unit only (fast)
pnpm run test:stories        # storybook/browser only (slower)
```

---

### 19. a11y gate

Every story is an accessibility test — structure, roles, names, keyboard reachability. The gate is configured globally in `.storybook/preview.ts`:

```typescript
parameters: {
    a11y: {
        test: 'error',                                    // axe violations FAIL the CI run
        config: { rules: [{ id: 'color-contrast', enabled: false },
                          { id: 'color-contrast-enhanced', enabled: false }] }
    }
}
```

**Colour contrast is deliberately NOT gated** — axe's two contrast rules are disabled globally and no other gate asserts contrast ratios. See `.claude/rules/visual-design-and-ui.md`; do not re-add a contrast gate, an APCA lint, or per-token contrast floors.

**The dark background decorator is mandatory**, not cosmetic: stories must render on the app's real dark surface, not on Storybook's default white, or every screenshot and review pass lies about how the component looks. The decorator paints `#0f1117` on the story root:

```typescript
decorators: [
    (story) => {
        document.documentElement.style.background = '#0f1117';
        document.body.style.background = '#0f1117';
        document.body.style.color = 'var(--text-1)';
        return story();
    }
]
```

**Per-story override** for intentionally broken states (demonstrating anti-patterns):

```svelte
<Story name="Anti-pattern" parameters={{ a11y: { test: 'todo' } }}>
```

Use `'todo'` (reported but not failing), not `'off'` (silenced entirely). `'off'` should never appear in production stories.

**axe is a floor, not a ceiling.** Manual keyboard and screen-reader testing is still required for novel interaction patterns.

---

### 20. TypeScript rules

`tsconfig.json` enables:

| Option | Effect |
|---|---|
| `"strict": true` | All strict checks: `strictNullChecks`, `noImplicitAny`, etc. |
| `"noUncheckedIndexedAccess": true` | Array indexing returns `T \| undefined`, not `T` |
| `"moduleResolution": "bundler"` | Vite-compatible resolution (no `.js` extension hacks) |

**eslint rules of note:**

- `typescript-eslint/recommended` — base TS-eslint rule set.
- `@typescript-eslint/no-unused-vars` — error, with `^_` prefix exemption for intentional unused params.
- `svelte/valid-compile` — error, but with `ignoreWarnings: true` because `custom_element_props_identifier` fires spuriously on `...restProps` patterns in a non-custom-element project.

**No `any`.** If you need an escape hatch, use `unknown` and narrow it explicitly. A cast to `as unknown as T` is a code smell; open a PR comment explaining why the type system can't be satisfied correctly.

**Svelte 5 runes only.** No Svelte 4 syntax:

| Banned | Replacement |
|---|---|
| `export let prop` | `let { prop } = $props()` |
| `$:` reactive statement | `$derived(...)` or `$effect(...)` |
| `<slot/>` | `{@render children()}` |
| `createEventDispatcher` | Callback props (`onclick`, `onchange`, etc.) |
| `on:click` | `onclick` |

---

### 21. How to add a new component

Exact six-step process:

**Step 1 — Write the story first.**

Create `src/lib/components/MyComponent.stories.svelte`. Use `defineMeta` from `@storybook/addon-svelte-csf`. Cover every meaningful state:

```svelte
<script module>
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import MyComponent from './MyComponent.svelte';

  const { Story } = defineMeta({
    component: MyComponent,
    title: 'Domain/MyComponent',
    tags: ['autodocs'],
    parameters: { a11y: { test: 'error' } },
  });
</script>

<Story name="Default" args={{ ... }}>...</Story>
<Story name="Empty" args={{ ... }}>...</Story>
<Story name="Error" args={{ ... }}>...</Story>
```

**Step 2 — Use design tokens, not raw values.**

Check `src/app.css` for the right token. If none fits, add one (see §14). Never write `color: #d5dae0` in a component — write `color: var(--text-1)`.

**Step 3 — Implement the component.**

Implement `MyComponent.svelte` so all stories render correctly. Rules:
- Use `$props()`, `$state`, `$derived`. No Svelte 4 syntax.
- Spread `...restProps` onto the root element (if it's a `ui/` primitive).
- Add `data-slot="my-component"` on each named sub-element.
- Keep it under ~150 lines. Extract sub-components when nearing that limit.

**Step 4 — Add a unit test for non-trivial logic.**

If the component contains reducers, formatters, or complex derived state, extract that logic into a `.ts` file and test it with Vitest unit tests. Pure rendering concerns are covered by stories.

**Step 5 — Verify a11y.**

Run `pnpm run test:stories`. Fix every axe violation before opening a PR. `a11y: { test: 'error' }` makes them CI-blocking.

**Step 6 — Wire into routes.**

Import the component in the appropriate route or parent component. `+page.svelte` files should contain only imports and data passing — no component logic.

---

## Quick reference

### Build and test

```bash
# Rust
make build          # cargo build --all
make test           # cargo nextest run --all
make lint           # Rust file-size + strict Clippy structural lint gate
make fmt            # cargo fmt
make check          # fmt + lint + test

# Frontend
cd frontend
pnpm run dev         # Vite dev (SPA only, no proxy)
pnpm run storybook   # Storybook on :6006 (the stack already runs it as mike-stack-storybook)
pnpm test            # Vitest (both projects)
pnpm run build       # Production SPA build
pnpm run lint        # eslint + svelte-check

# Full stack
./scripts/start-proxy.sh    # Rust proxy + Vite dev SPA (ports 9988/9989/9990)
./scripts/stop-proxy.sh     # Graceful shutdown of all processes
```

### Ports

| Port | Service |
|---|---|
| 9988 | Proxy bind — `ANTHROPIC_BASE_URL=http://localhost:9988` |
| 9989 | Inspect bind — REST API + WebSocket |
| 9990 | SPA host (Vite dev or `pnpm run preview`) |
| 6006 | Storybook |

### Key files

| File | Purpose |
|---|---|
| `docs/ARCHITECTURE.md` | Full rationale, decision log, request lifecycle, data model — 1154 lines |
| `docs/REQUEST_FLOW.md` | Annotated request lifecycle (line-cited) |
| `docs/SCHEMA.md` | Full DB schema reference |
| `crates/mike/src/api/manifest.rs` | Inspect route + MCP coverage registry |
| `crates/mike-mcp/tests/manifest_coverage.rs` | Parity test (both directions) |
| `crates/mike/src/store/persistence/migrations/` | sqlx migration files |
| `frontend/src/app.css` | All design tokens |
| `frontend/docs/frontend/STYLE_GUIDE.html` | Design export FUNK started from (reference; the style guide is Storybook's Foundations stories) |
| `/settings` | Live runtime configuration surface and source of truth |
| `.claude/rules/agent-behavior-and-data.md` | No-truncation rule — read before touching capture/projection |
