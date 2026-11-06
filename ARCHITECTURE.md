# mike Architecture

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
├── mike/           — The proxy, inspect and SPA binds in one binary.
│   src/            Three axum listeners sharing one AppState (proxy :9988,
│                   inspect :9989, SPA :9990). Owns the capture pipeline,
│                   SSE tee, filter, composer, weasel intercept, DB
│                   writes, WebSocket broadcast, all REST routes, the
│                   terminal UI and the `mike spawn` launcher.
│
├── mike-mcp/       — Separate MCP bridge binary.
│   src/            Spawned by Claude Code over stdio. Talks HTTP to
│                   the inspect bind (:9989). Exposes a `core` and a `full`
│                   tool profile plus 3 resources via rmcp 3.1 (run
│                   `mike-mcp list-tools --core --json` or `--full --json`
│                   for the authoritative catalogue). NEVER imports
│                   mike as a library crate.
│
└── proxy-forward/  — Separate HTTPS_PROXY MITM sidecar (:9987).
    src/            Lets Claude Code keep Remote Control while mike
                    captures; fails over to Anthropic when mike is down.
```

**One process, three listeners.** `mike serve` runs three `axum::serve` futures sharing one `AppState` (`crates/mike/src/serve/listeners.rs`):

| Bind | Port | Routes | Contract |
|---|---|---|---|
| Proxy | 9988 | `/v1/*` and `/v2/*` → `api.anthropic.com` | Injecting/optionally-mutating pipeline (filter rules, lesson/reminder/steerer injection, composer splicing, weasel intercept); identity is the no-op default when no rule matches — capture always stores the untouched inbound bytes |
| Inspect | 9989 | `/api/*` REST + `/api/ws` WS | Host/Origin allow-list (`api/host_guard.rs`) plus CORS, compression, request-id, timeout and trace layers; no authentication |
| SPA | 9990 | the whole `/api/*` router merged with the embedded SvelteKit build | Enabled by default (`spa_enabled`); the development stack turns it off and runs `vite dev` on the port instead |

**Process layout.** The development stack (`scripts/start-proxy.sh`) runs `mike` with `MIKE_SPA_ENABLED=0`, the SPA as an independent `vite dev` process on `:9990`, the `proxy-forward` sidecar on `:9987`, the prompt assistant (`services/prompt-assistant`) on `:9985`, Storybook on `:6006` and the architecture diagrams on `:9986`, each a `mike-stack-*` systemd user unit. The MCP bridge is a separate binary (`mike-mcp`) spawned by Claude Code. An installed or plain `mike` launch serves the built SPA from the binary instead (`crates/mike/src/spa.rs`).

**Workspace-level dependencies** are pinned in the root `Cargo.toml` `[workspace.dependencies]` — add a dep there first, then refer to it with `{ workspace = true }` in the crate manifest. This prevents version drift between crates.

#### Module map (`crates/mike/src/`)

| Module | Purpose |
|---|---|
| `proxy/` | Core proxy domain. `handler.rs` is the axum handler for `/v1/*` and `/v2/*`; `engine/` holds the pipeline steps (`accept`, `run_request_pipeline`, `forward_to_upstream`, `finalize_and_respond`). `upstream.rs` handles reqwest forwarding and URI rewriting, `headers.rs` is pure header filtering/rewriting, `response/` the tee and buffered forwarders, `remote_control/` the Remote Control relay. |
| `pipeline/` | The `RequestStage` abstraction and `STAGE_ORDER`, the declared order of the mutating stages. Each domain module contributes its own `stage.rs`. |
| `extract/` | Pure functions for request/response body inspection. No async, no state, fully unit-testable. |
| `sse/` | SSE parsing and tee. `tee.rs` is the channel-based byte-tee. `parser/` emits `ProxyEvent`s. |
| `store/` | In-memory request store. `persistence/` holds Postgres write paths and `migrations/`. `operator_turns.rs` manages the OPEN→CLOSED lifecycle. |
| `api/` | Inspect bind REST/WS. `manifest.rs` builds the router and declares each route's MCP coverage; `manifest/` holds the per-group route tables. `handlers/` holds per-resource REST handlers. |
| `serve/` | Server lifecycle: listeners, startup sequence, background workers, shutdown. |
| `filter/` | System-prompt filter pipeline. Drop/replace/append rules against `system[]` and `messages[*].content[]`. `FilterConfig` is hot-swapped via `ArcSwap`. |
| `compose_prompt/` | Prompt composition pipeline. Runs after the stages. Byte-splice (not parse-and-reserialize) to preserve cache keys. |
| `reminders/` | Appends `<system-reminder>` blocks (session lessons, analyzer findings, change-record tail, git read hint) to outgoing requests. |
| `weasel_intercept/` | Streaming gate over the SSE reply. Cuts the reply at a listed phrase or at a proxy-declared tool call and continues it with a second upstream answer. |
| `agents/` | Agent registry domain. `AgentSource` enum (Observed, FileBacked, Dynamic). |
| `analysis/` | Diagnostic pattern analysis. 19 patterns across 5 categories; reporting only, never injected. |
| `config/` | `ProxyConfig` + figment 4-layer pipeline (defaults, config file, `MIKE_*` env, CLI). `AppState` construction. `paths.rs` resolves the installation's directories. |
| `diagnostics/` | Process-level diagnostics wrapper around the `analysis/` analyzers. |
| `observability/` | Structured event emission, the startup banner and the memory ticker. |
| `memory_cap/` | Global RSS cap manager. `Sheddable` trait + `ShedRegistry`. Background RSS tick loop. |
| `in_flight.rs` | In-flight request registry. DashMap-backed with RAII `InFlightGuard`. Hard cap returns HTTP 503. |
| `retention_worker.rs` | DB-side TTL sweep on a timer, gated by the `retention_sweep` toggle. |
| `db_listener/` | Postgres LISTEN/NOTIFY on `proxy.row_changed`. Re-broadcasts as `ProxyEvent::RowChanged`. |
| `startup.rs` | Boot-time recovery and warm-up: stale-row recovery, mailbox reload, session sweep. |
| `main.rs` | CLI parse and subcommand dispatch. |

Per-domain detail is in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) §9, which has the fuller module map.

---

### 2. Axum handler structure

#### Thin handlers, no Tower middleware on the body path

The proxy handler (`proxy/handler.rs`) is **not** Tower middleware. The proxy router carries no Tower layers at all (`app.rs`, `proxy_router`: two routes and `with_state`); the inspect and SPA routers carry the middleware (`api/manifest.rs`, `local_router`: the Host/Origin guard, CORS, compression, request ids, tracing and timeouts). The handler hands the request to `proxy::engine`, which runs the pipeline in sequence:

```
accept: extract parts + RequestSignals, keep the ORIGINAL bytes
  → mutation stages        (pipeline::STAGE_ORDER: filter, reminders, steerer drain,
                            compression, proxy_tools, ... — each behind its own gate)
  → compose_prompt pipeline (byte-splice system[2] for first-of-session)
  → capture admission guard
  → build InterceptedRequest
  → insert_request_initial   ← Phase 1 DB write (stub row)
  → in_flight.register()     → HTTP 503 if at capacity
  → client.post(upstream).send()
  → tee SSE stream           (or buffer non-streaming)
  → snapshot_request()       ← Phase 2 DB write (terminal UPDATE)
  → NOTIFY proxy.row_changed
```

The stage list, gates and defaults are in [`docs/REQUEST_FLOW.md`](docs/REQUEST_FLOW.md) and [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) §3–4.


The inspect bind (`api/manifest.rs`, `inspect_router`) builds a separate axum `Router`. Every handler function is a thin dispatcher: read from `AppState`, call a service/persistence function, return a typed response.

#### Adding a new inspect route

Two coordinated edits are required — a missing one fails a test:

1. **Add the route** in the group's module under `crates/mike/src/api/manifest/<group>.rs`: an entry in its `ROUTES` table (method, path, `Coverage`) and the `.route(...)` line in its `register` function, pointing at a handler in `api/handlers/`. A new group module is also listed in `inspect_route_manifest()` and `local_router()` in `api/manifest.rs`.
2. **Choose its `Coverage`:**
   - `Coverage::Tool("tool_name")` — wraps a named MCP tool 1:1.
   - `Coverage::AggregatedInto("tool_name")` — data folded into an aggregate tool.
   - `Coverage::Resource("proxy://uri")` — surfaced as an MCP resource.
   - `Coverage::Exempt { reason: "..." }` — deliberately not exposed via MCP. This is the default for a new route: every MCP tool costs tokens on every request, so a tool is added only when a real workflow needs one (in `crates/mike-mcp/src/tools/`).

The parity test at `crates/mike-mcp/tests/manifest_coverage.rs` walks the manifest and fails if a declared tool does not exist, and fails for a tool that no manifest entry points at.

---

### 3. MCP crate (mike-mcp)

**The MCP bridge is a thin HTTP client, not a shared library.** It has no `Cargo.toml` dependency on `mike`'s binary code. It talks HTTP to the inspect bind on `:9989`. Tool implementations live in `crates/mike-mcp/src/tools/<domain>.rs`; each parses its arguments, calls one inspect endpoint through `InspectClient` and returns the result (every tool also accepts a `select` projection, applied in `tools/envelope.rs`).

**No business logic lives here.** If a tool does more than deserialize arguments, call one HTTP endpoint, and return the result, the logic belongs in the inspect bind, not the MCP bridge.

#### Adding an MCP tool

1. Add the tool in the matching `crates/mike-mcp/src/tools/<domain>.rs`: a `TOOL_<NAME>` constant, its `Tool` definition and its handler branch, and re-export the constant from `tools.rs`.
2. Register it in the domain's tool list so `tools/catalogue.rs` (`all_tools`) and `tools/dispatch.rs` (`call_tool`) reach it. Add its name to `CORE_TOOL_NAMES` only if you would reach for it while investigating a session; otherwise it ships in the `full` profile only.
3. Add the matching `Coverage::Tool("tool_name")` entry in the route's `api/manifest/<group>.rs` table.
4. Run `scripts/cx nextest run -p mike-mcp --test manifest_coverage` — the parity test passes once both sides agree.

**All tracing goes to stderr.** Stdout is the JSON-RPC wire — a stray log line corrupts the MCP protocol.

---

### 4. The `test-seams` feature flag

Some integration tests need to inject a wiremock server URL as the upstream, bypassing the real `api.anthropic.com`. The `test-seams` feature on the `mike` crate (declared in `crates/mike/Cargo.toml`) enables compile-time seams for this substitution.

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

**Gate command:** `scripts/lint-rust` (`make lint`)

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

Config loads in a 4-layer `figment` pipeline. Later layers win:

```
defaults  →  config.toml  →  MIKE_* env vars  →  CLI flags
```

The config file is the installation's `config.toml` (`mike config path` prints it; `--config <path>` selects another). The binary does not load a `.env`; import one with `mike import-dotenv`. The live operator source of truth is `/settings`.

**Operator CLI surface:** Two subcommands are the main ones for inspecting boot config out-of-process (`mike --help` lists the rest):
- `mike print-config [--shell]` — resolved effective values (the `LIVE OVERLAY` section shows what a running session actually got)
- `mike migrate-check` — validate the current database/config state before startup

Boot config precedence is defaults < config file < `MIKE_*` env < CLI flags. [`docs/CONFIGURATION.md`](docs/CONFIGURATION.md) is the reference.

**`scripts/start-proxy.sh` sources the binary:**

```bash
eval "$(mike print-config --shell)"
```

The shell script has no config knowledge of its own. If you add a config key, add it to the `print-config --shell` output — the script picks it up automatically.

The runtime settings catalog is edited in `/settings`. Filter rules are **not** a config surface — they are DB-backed and authored at runtime via the inspect API / MCP `*_filter_rule` tools.

---

### 8. Four-tier test taxonomy

Tests are split across named `[[test]]` binary targets in `crates/mike/Cargo.toml` (`proxy`, `integration`, `e2e`, `cli`, `schema`, `reminder_on_token` and several `*_contracts` / `release_*` targets) so nextest can run them in parallel:

| Tier | Named target | What it covers | DB? | Upstream? |
|---|---|---|---|---|
| **Proxy** | `proxy` | Handler pipeline, SSE tee, filter, weasel intercept, config (wiremock-only tests in `crates/mike/tests/proxy/`) | No | Wiremock (no real Anthropic) |
| **Integration** | `integration` | DB writes, persistence, migration round-trips, operator turn lifecycle | Real Postgres | No |
| **Schema** | `schema` | Migration checksums — fails if a migration file changes after merging | Real Postgres | No |
| **E2E** | `e2e` | Full system: proxy bind + inspect bind + DB + real SSE stream | Real Postgres | Wiremock |

Unit tests live beside the code in `src/`. `.config/nextest.toml` partitions the suite: the default profile runs the tests that need no database, the `db` profile the rest, and `ci` all of them.

**nextest runs the targets in parallel** across separate processes. Cross-binary env races are impossible because each binary is a separate OS process with its own env.

**`serial_test` for env-mutating tests.** Tests that mutate process-global env (`MIKE_CONFIG`, `HOME`, `XDG_CONFIG_HOME`) within a single binary use `#[serial_test::serial(<group>)]`. This gates those tests within their binary without serialising the whole workspace.

**Never `RUST_TEST_THREADS=1`.** It serialises the entire workspace; per-binary `#[serial]` is the correct fix.

**`make test`** runs the no-database tests (`scripts/cx nextest run --all`), `make test-db` the database partition and `make test-all` everything. `make test-p p=<crate>` scopes to one crate. `make test-t t=<filter>` runs a single test by name pattern.

---

### 9. Database

**sqlx with compile-time query checks.** All queries use `sqlx::query!` or `sqlx::query_as!` macros which verify SQL at compile time against the target schema. Normal builds use the committed `.sqlx/` offline cache — no DB connection and no `DATABASE_URL`. After adding/changing a query, add it to the cache with `scripts/sqlx-prepare` (reads `MIKE_DATABASE_URL`, the repo's only DB var). Never run `cargo sqlx prepare` directly: it empties `.sqlx/` while it runs and breaks every build in the checkout.

**Migrations.** `store/persistence/migrations.rs` embeds `store/persistence/migrations/` with `sqlx::migrate!`. Only `mike serve` applies them; other subcommands refuse to run while the migration ledger differs from the binary. The proxy never runs against an unmigrated schema.

**Test database.** DB-backed tests provision ephemeral databases (`claude_proxy_test_<uuid>`) on the Postgres that `MIKE_TEST_ADMIN_URL` names; `scripts/test-db/` starts a throwaway container for it (`mike-test-pg` on `127.0.0.1:6667`). Tests never touch the capture database.

**Test DB isolation.** Use `TestDb` from `crates/mike/src/store/persistence/test_support/`. `TestDb::acquire` hands out a shared database: own your rows with unique ids, and do not assert global counts, truncate tables or issue unscoped deletes. Use `TestDb::acquire_isolated` when a test needs an empty private database, and call `handle.cleanup().await`.

**Schema tests (`schema` target).** After a migration is merged, its checksum is fixed. `crates/mike/tests/schema/migration_checksums.rs` compares every file in `store/persistence/migrations/` with `migration-baseline.json`. This catches the "I edited a migration that's already applied" class of mistake early.

**Two-phase write.** Every captured request goes through two DB writes:
1. `insert_request_initial` — stub `Pending` row before upstream send. Enables FK-valid content-block inserts during streaming.
2. `snapshot_request` — terminal transaction at completion: request status, the `responses.responses` row and diagnostics.

Rows stuck as `Pending` after a crash are healed by `recover_stale_pending_rows()` on the next startup.

---

### 10. Script conventions

The `scripts/` directory contains the full operational toolchain. All scripts are idempotent and safe to re-run.

#### Port table

| Port | Service | Purpose |
|---|---|---|
| 9988 | Proxy bind | `/v1/*` and `/v2/*` — set `ANTHROPIC_BASE_URL=http://127.0.0.1:9988` |
| 9989 | Inspect bind | `/api/*` REST + `/api/ws` WebSocket + CORS |
| 9990 | SPA host | SvelteKit SPA (`vite dev` with HMR in the development stack; served by `mike` itself otherwise) |
| 9987 | proxy-forward | HTTPS_PROXY MITM sidecar |
| 9986 | Diagrams | The LikeC4 architecture model (`scripts/arch`) |
| 9985 | Prompt assistant | `services/prompt-assistant` |
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
STORYBOOK_PORT=6007 ./scripts/stop-proxy.sh   # custom Storybook port
```

#### `scripts/install.sh`

Interactive one-shot setup wizard for a source checkout. Walks through: prerequisite checks (Rust, Node), Rust workspace build, frontend dep install, storage mode choice (in-memory / external Postgres), surgical `.env` bootstrap update (touches only the five managed keys, shows diff, asks before writing), and MCP bridge snippet for `~/.claude.json`. Safe to re-run. Released builds install through `mike setup` instead.

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

1. Choose a filename with a 14-digit UTC timestamp prefix that sorts after every existing migration (`YYYYMMDDHHMMSS_description.sql`, for example `20261103150000_parallel_autovacuum.sql`).
2. Prefer idempotent SQL (`CREATE TABLE IF NOT EXISTS`, `ADD COLUMN IF NOT EXISTS`, `CREATE OR REPLACE`), since migrations run against existing databases.
3. Put the object in its domain schema (`requests`, `sessions`, `memory`, `tools`, `steering`, `launch` and so on; [`docs/SCHEMA.md`](docs/SCHEMA.md) says which table lives where), not a catch-all schema.
4. Prefer additive changes. Never DROP a column in the same migration that removes its usage — separate those into different deploys.

#### Migration naming convention

```
YYYYMMDDHHMMSS_description.sql
```

- `YYYYMMDDHHMMSS` — UTC timestamp
- `description` — snake_case, short (e.g., `add_embedded_at_to_operator_turns`, `parallel_autovacuum`)

The lexicographic order of filenames IS the migration execution order. Never rename a migration file after it has been merged to main — schema tests checksum every file and fail on any change.

#### Refreshing the sqlx offline cache

`sqlx::query!` macros verify SQL at compile time. When you add or change a query, refresh the offline cache so builds work without a live DB:

```bash
scripts/sqlx-prepare            # reads MIKE_DATABASE_URL from .env; adds the new queries to .sqlx/
```

Commit the `.sqlx/` changes with the migration and the code that uses it. Do not run `cargo sqlx prepare` directly (see §9).

#### Testing with the schema test tier

After writing a migration:

1. Run the schema target: `make test-t t=migration_checksums`. It compares every migration with a frozen baseline and needs no database; new migrations are accepted, a changed or removed historical one fails.
2. If you see a checksum failure, you edited a migration that was already applied. Fix the issue in a NEW migration file, never by editing the existing one, and never regenerate the frozen manifest to silence the check.
3. The schema target runs in the landing gate.

#### When the proxy is running against an existing DB

Migrations run automatically on the next `mike serve` start (`store/persistence/migrations.rs`). For a large migration (e.g. backfilling a new column across millions of rows), either:
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

**Themes.** The app ships five themes (`forge`, the default, `glacier`, `void`, `phosphor`, `crimson`; `src/lib/themes.ts`), each a `[data-theme='…']` block of token values in `src/app.css`. Components read `var(--*)` tokens, so they follow whichever theme is active.

**Convention:** outside `components/ui/`, application components never use `dark:` prefixes. The `@custom-variant dark` directive exists for the `shadcn-svelte` primitives under `ui/`.

#### Adding a new design token

1. Add `:root { --my-token: ...; }` declaration in `app.css`.
2. Add `@theme inline { --color-my-token: var(--my-token); }` if you want a Tailwind utility (`bg-my-token`).
3. Never reach for an ad-hoc hex value in a component file. If a token doesn't exist, add it here.

#### Available token categories

| Category | Examples |
|---|---|
| Surfaces | `--bg`, `--surface-bg`, `--surface`, `--surface-2`, `--sidebar` |
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

`.storybook/stub-aliases.ts` (`libStubAliases`) lists the module paths to replace, and both `.storybook/main.ts` and `vitest.config.ts` apply it: when a story imports `$lib/ws.svelte`, it gets `$lib/__storybook-stubs__/ws.svelte.ts` instead.

**Order matters.** More-specific paths must come before prefix matches (the real list puts `$lib/api/<module>` before the bare `$lib/api` stub):

```typescript
{ find: /^\$lib\/api\/(.+)$/, replacement: path.join(root, 'src/lib/api/$1.ts') },
{ find: '$lib/api', replacement: stub('api.ts') }
```

#### Adding a stub for a new singleton

1. Create `src/lib/__storybook-stubs__/my-singleton.svelte.ts` exporting the same interface as the real module but with static/no-op implementations.
2. Add the alias to `libStubAliases` in `.storybook/stub-aliases.ts`:
   ```typescript
   { find: '$lib/my-singleton.svelte', replacement: stub('my-singleton.svelte.ts') }
   ```
3. Verify in Storybook that stories using the stubbed module render without errors.

**Stubs are for isolation only.** They must not contain production logic — any shared logic belongs in the real module, not the stub.

---

### 18. Vitest two-project setup

`vitest.config.ts` defines three named test projects:

| Project | Environment | What it tests | File pattern |
|---|---|---|---|
| `unit` | node | Pure TS functions, store reducers, formatters, validators; external I/O mocked | `src/**/*.{test,spec}.{js,ts}` (excludes `*.integration.*`) |
| `integration` | node | Code against real IndexedDB, fetch and WebSocket | `src/**/*.integration.{test,spec}.{js,ts}` |
| `storybook` | browser/chromium (Playwright) | Stories rendered in real Chromium: smoke, interaction, a11y | Stories discovered by the `storybookTest` plugin |

**`vitest.config.ts` is separate from `vite.config.ts`.** The SvelteKit Vite config (`vite.config.ts`) is for the app build. The Vitest config is for tests only. Mixing them causes the `storybookTest` plugin to interfere with the build.

**Which project for new tests:**

- Pure function → `unit` project. Filename `*.test.ts`.
- Component rendering / interaction / a11y → `storybook` project. Write a story (see §13), not a separate test file.
- Full multi-route user flow → Playwright (`e2e/`, `pnpm run test:e2e`).

**Running tests:**

Run frontend tests through `scripts/sb-test`, not raw `vitest`:

```bash
scripts/sb-test --skip-typecheck <filter>   # a focused run
pnpm run test:unit                          # unit project (wraps scripts/sb-test --unit)
pnpm run storybook:test                     # storybook project (wraps scripts/sb-test --storybook)
```

---

### 19. a11y gate

Every story is an accessibility test — structure, roles, names, keyboard reachability. The gate is configured globally in `.storybook/preview.ts`:

```typescript
parameters: {
    a11y: {
        test: 'error',                                    // axe violations FAIL the run
        options: { rules: { 'color-contrast': { enabled: false },
                            'color-contrast-enhanced': { enabled: false } } }
    }
}
```

**Colour contrast is deliberately NOT gated** — axe's two contrast rules are disabled globally and no other gate asserts contrast ratios. Do not re-add a contrast gate, an APCA lint, or per-token contrast floors; judge legibility in the running app.

**Stories render under a theme decorator**, not on Storybook's default white: `.storybook/preview.ts` wraps every story in `withThemeByDataAttribute` with `data-theme` set (default `forge`), so screenshots and review passes show the component as the app draws it.

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

Check `src/app.css` for the right token. If none fits, add one (see §14). Never write a raw hex value in a component — write `color: var(--text-1)`.

**Step 3 — Implement the component.**

Implement `MyComponent.svelte` so all stories render correctly. Rules:
- Use `$props()`, `$state`, `$derived`. No Svelte 4 syntax.
- Spread `...restProps` onto the root element (if it's a `ui/` primitive).
- Add `data-slot="my-component"` on each named sub-element.
- Keep it under ~150 lines. Extract sub-components when nearing that limit.

**Step 4 — Add a unit test for non-trivial logic.**

If the component contains reducers, formatters, or complex derived state, extract that logic into a `.ts` file and test it with Vitest unit tests. Pure rendering concerns are covered by stories.

**Step 5 — Verify a11y.**

Run `pnpm run storybook:test`. Fix every axe violation before opening a PR. `a11y: { test: 'error' }` makes them CI-blocking.

**Step 6 — Wire into routes.**

Import the component in the appropriate route or parent component. `+page.svelte` files should contain only imports and data passing — no component logic.

---

## Quick reference

### Build and test

```bash
# Rust
make build          # cargo build --all
make test           # no-database tests (cargo nextest)
make test-db        # database tests
make lint           # Rust structural lint gate
make fmt            # cargo fmt

# Frontend
cd frontend
pnpm run dev         # Vite dev server with HMR (:9990)
pnpm run storybook   # Storybook on :6006 (the stack already runs it as mike-stack-storybook)
pnpm run test:unit   # unit tests, through scripts/sb-test
pnpm run build       # Production SPA build
pnpm run lint        # eslint

# Full stack
./scripts/start-proxy.sh    # builds and deploys the proxy, sidecar, assistant, web UI, Storybook, diagrams
./scripts/stop-proxy.sh     # stops the mike-stack-* units
```

### Ports

| Port | Service |
|---|---|
| 9988 | Proxy bind — `ANTHROPIC_BASE_URL=http://localhost:9988` |
| 9989 | Inspect bind — REST API + WebSocket |
| 9990 | SPA host (Vite dev in the development stack, otherwise `mike` itself) |
| 9987 | proxy-forward sidecar |
| 9986 | Architecture diagrams |
| 9985 | Prompt assistant |
| 6006 | Storybook |

### Key files

| File | Purpose |
|---|---|
| `docs/ARCHITECTURE.md` | Full rationale, decision log, request lifecycle, data model |
| `docs/REQUEST_FLOW.md` | Annotated request lifecycle (symbol-cited) |
| `docs/SCHEMA.md` | Full DB schema reference |
| `crates/mike/src/api/manifest.rs` | Inspect route + MCP coverage registry |
| `crates/mike-mcp/tests/manifest_coverage.rs` | Parity test (both directions) |
| `crates/mike/src/store/persistence/migrations/` | sqlx migration files |
| `frontend/src/app.css` | All design tokens |
| `docs/frontend/STYLE_GUIDE.html` | Design export FUNK started from (reference; the style guide is Storybook's Foundations stories) |
| `/settings` | Live runtime configuration surface and source of truth |
| `docs/decisions/inspection-fidelity-catalogue.md` | Inspection-fidelity patterns — no truncation of captured content; read before touching capture/projection |
