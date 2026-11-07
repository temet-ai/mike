---
title: Troubleshooting
status: active
created: 2026-06-17
updated: 2026-11-07
summary: Fixes for common local build and runtime failures — build accelerators, linker errors, toolchain PATH, DB reachability, port conflicts, inspect-API Host/Origin 403s, MCP connection errors, and stale frontend state.
---

# Troubleshooting

Common local failures and their fixes. Most build/run paths self-heal via
`scripts/lib/preflight-toolchain.sh` (run on every `cx`, `make`, and
`start-proxy.sh`/`deploy-proxy.sh` build); the entries below explain what that
preflight does and how to recover when it can't.

## Build & Toolchain

### Build accelerators missing (sccache / lld)

`.cargo/config.toml` is committed and is **not** empty or stock. Under
`[target.'cfg(all())']` it sets global rustflags denying `clippy::unwrap_used`,
`clippy::expect_used`, `clippy::complexity`, `clippy::cognitive_complexity`,
`clippy::too_many_arguments` and `clippy::too_many_lines`, so those thresholds
apply to every cargo invocation in this repo — including a plain `cargo build`.
What it deliberately does not carry is any build accelerator and any database
URL; its own comment says never to commit a maintainer-specific
`MIKE_TEST_ADMIN_URL` default. A fresh clone therefore builds with plain
`cargo` and no host tools.

`scripts/cx` and the Makefile `build`/`test`/`lint`/`install` targets opt in to
two host tools by injecting `cargo --config` at invocation time (see
`compute_accelerator_cargo_args` in `scripts/lib/preflight-toolchain.sh`):

- **sccache** — `build.rustc-wrapper`, all platforms. Injecting it also sets
  `build.incremental=false`, since sccache and incremental compilation are
  incompatible.
- **lld** — the linker, on macOS targets only. On Linux the preflight never
  asks for lld at all, so only sccache can be missing there.

#### Fix

On macOS, with Homebrew:

```bash
make bootstrap        # installs both via Homebrew, idempotent
# equivalently:
brew install sccache lld
```

Without Homebrew — which is every Linux host — the preflight cannot install
anything and prints the manual path instead, then returns non-zero:

```bash
cargo install sccache     # or your distro's package, e.g. pacman -S sccache
```

lld is only needed for macOS Mach-O linking; on Linux there is nothing to
install for it. The other option the preflight offers is to skip the
accelerators entirely by running plain `cargo` instead of `scripts/cx` or
`make` — `.cargo/config.toml` never requires them. (Plain `cargo` does not
preload `.env`; see the capture-DB section below.)

`scripts/cx` and the accelerator-carrying `make` targets all run the
preflight automatically and will install missing tools when Homebrew is present,
then inject only the accelerators actually found — a missing tool is skipped,
not fatal, since these paths never touch the committed config. To opt out of
auto-install (e.g. CI that provisions its own toolchain) set
`MIKE_SKIP_TOOLCHAIN_BOOTSTRAP=1`, which downgrades the missing tools to a
warning and continues.

#### Symptoms

- **sccache absent:**
  `error: process didn't start: ... sccache: No such file or directory`
- **lld absent / wrong driver** (see next section).

### `ld.lld: error: unable to find library -lSystem` on macOS

You enabled lld but pointed at the **wrong driver**. `ld.lld` is lld's ELF/GNU
driver; forcing macOS to link Mach-O through it fails with:

```
ld.lld: error: unable to find library -lSystem / -lc / -lm
ld.lld: warning: archive member '...' is neither ET_REL nor LLVM bitcode
```

#### Fix

macOS Mach-O needs **`ld64.lld`**, the Mach-O driver (same lld binary, different
symlink). `scripts/lib/preflight-toolchain.sh` already injects it for
`scripts/cx` and the accelerator-carrying `make` targets:

```
--config target.aarch64-apple-darwin.rustflags=["-C","link-arg=-fuse-ld=/opt/homebrew/opt/lld/bin/ld64.lld"]
```

Two related traps:

- lld ships in the standalone **`brew install lld`** formula — the `llvm`
  formula no longer bundles it.
- lld is keg-only, so the injected flag uses an **absolute path**. If `brew
  --prefix lld` differs from the path `_expected_lld_path()` computes
  (unusual), update that function in `scripts/lib/preflight-toolchain.sh` —
  the preflight prints both paths when it detects a mismatch.

A trivial single-file `rustc` link can succeed even with the broken `ld.lld`, so
only the real workspace build (build scripts + rlibs) exposes the failure — test
the fix with an actual `cargo build -p mike`, not a toy crate.

### `cargo: command not found` from scripts (rustup not on PATH)

Rustup installs `cargo` to `~/.cargo/bin` and relies on a shell-profile line to
add it to `PATH`. Non-login shells (Claude Code, CI, `cron`) often don't source
that line. `start-proxy.sh` and `deploy-proxy.sh` already prepend
`~/.cargo/bin` silently; if you hit this in another context, add it to PATH:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

### Capture DB unreachable (`connection refused`, DB-backed test / `start-proxy.sh` failures)

**Compiling needs no database connection, but raw `cargo` still needs the repo's env.** The sqlx
`query!`/`query_as!`/`query_scalar!` macros verify against the committed `.sqlx/` offline cache,
and there is no `DATABASE_URL` env var in this repo. What a bare `cargo build` in your shell
*does* lack is everything the untracked `.env` carries — `MIKE_DATABASE_URL`,
`MIKE_TEST_ADMIN_URL` and the rest — so any DB-backed test fails on an unset variable. Use
`scripts/cx <cargo args>` instead: it resolves the repo root from its own location (so it runs
from any cwd), sources `.env`, and builds into the checkout's own `target/` unless
`CARGO_TARGET_DIR` is set.

A live Postgres is required for two things: `cargo sqlx prepare` (to regenerate the `.sqlx` cache
after changing a query — needs `MIKE_DATABASE_URL`), and DB-backed tests (need
`MIKE_TEST_ADMIN_URL`, which clones an ephemeral per-test DB from a migrated template). Both
variables live in `.env`; neither is in `.cargo/config.toml`.

`start-proxy.sh` needs a reachable DB at runtime (persistence defaults ON), and it does **not**
start Postgres for you. Its preflight is not a plain exit-on-failure:

1. It TCP-probes the host and port parsed out of `MIKE_DATABASE_URL`. Reachable — it continues.
2. If that fails, it probes `MIKE_LOCAL_DATABASE_URL` as well, provided one is configured and
   `MIKE_DB_FALLBACK_ENABLED` is not `0`. If the local fallback answers, it prints that the
   proxy will fall back to it and continues — no prompt, no failure. See
   [local DB fallback](LOCAL_DB_FALLBACK.md).
3. With neither reachable, it asks on the terminal: `[r]` retry the probe after a pause, `[s]`
   start anyway (the build and smoke test run first, so the DB has until the smoke test to come
   back, and the deploy fails if it does not), or `[a]` abort. Enter means abort.
4. With no terminal to ask on — CI, a non-interactive agent — it takes the abort path and exits 1.

A development checkout on the same host as the stack reaches its database through `.env`. For an
installation-owned local PostgreSQL, run `mike db up` (Docker), or
`mike --config PRIVATE_CONFIG database provision`, or the thin
`scripts/local-db/up.sh --config PRIVATE_CONFIG` development wrapper. The Compose resource is
embedded and only `mike --config PRIVATE_CONFIG serve` runs migrations. See
[`INSTALLED_DATABASE.md`](INSTALLED_DATABASE.md). `docker-compose.dev.yml` in the repo root
still defines a `postgres` service under the `db` profile; there is no root
`docker-compose.yml`, so a bare `docker compose up -d postgres` fails with "no configuration
file provided" — pass `-f docker-compose.dev.yml --profile db`.

If queries fail with `connection refused` while Docker reports the container "healthy", check
host **disk space**: a full disk produces I/O errors that kill the DB port even though the
container looks up. The cargo build directory (`target/` in each checkout, or the directory
`CARGO_TARGET_DIR` names) is the usual culprit. `scripts/cx` reclaims space when free space is
low; to reclaim it by hand, clear old artifacts rather than running `cargo clean`, which forces
a cold rebuild.

### Database unreachable while mike runs

`GET /api/health` reports `persistence: {enabled: true, reachable: false}` and a top-level
`status: degraded`; DB-backed tests fail in bulk with `pool timed out while waiting for an open
connection`. A `degraded` payload whose only cause is the `llm` block is a separate condition;
read `persistence.reachable` to judge whether capture itself is healthy.

Check the database first (`mike doctor`, then `mike db status` for the Docker database, or
`docker logs` on its container for crash recovery, a checksum failure or a full disk). Start it
again with `mike db up`, which keeps the data. Do not restart `mike` and do not re-run any
migration: the connection pool reconnects and `persistence.reachable` returns to `true` within
seconds. A machine that cannot always reach its database can use
[local DB fallback](LOCAL_DB_FALLBACK.md).

## Runtime

### `Address already in use` on `:9988`, `:9989`, or `:9990`

`mike` itself opens three listeners: `:9988` the proxy bind, `:9989` the inspect bind, and
`:9990` the SPA bind. `spa_enabled` defaults **on**, and the SPA bind is not static-file-only —
it serves the whole `/api/*` router as well, so `:9990` answers inspect API calls too.

In development `:9990` is usually `vite dev` rather than mike: `scripts/start-proxy.sh` exports
`MIKE_SPA_ENABLED=0` precisely so the embedded SPA does not take the port the dev server wants.
That is why the same port means different things depending on how the stack was started.

The launcher auto-kills whatever holds those ports before binding. If it can't (permissions, a
different process holding the port on a locked-down machine), move the affected service:

| Service | Env | CLI flag |
|---|---|---|
| Proxy bind | `MIKE_LISTEN_ADDR` | `--proxy-listen-addr` |
| Inspect bind | `MIKE_INSPECT_ADDR` | `--inspect-listen-addr` |
| mike's embedded SPA bind | `MIKE_SPA_BIND` | `--spa-bind` |
| `vite dev` and the scripts | `MIKE_FRONTEND_PORT` | `--frontend-port` |

`MIKE_FRONTEND_PORT` does **not** move mike's own SPA listener — it is what the scripts and the
dev server read. Use `MIKE_SPA_BIND` for the embedded one, or `MIKE_SPA_ENABLED=0` to not bind
it at all.

Or free the port manually:

```bash
lsof -ti :9988 | xargs kill
```

### `the selected installation already has a daemon owner; use its private control connection`

`mike serve` takes a lock file, `daemon.lock` in the state folder (`~/.local/state/mike`), before it
binds any port, so two proxies cannot run for one installation. This message means another process
of the same installation holds that lock. When mike was set up with `mike service install`, that
process is the installed background service: launchd or systemd restarts it within seconds of
anything stopping it, and it takes the lock before a proxy you started yourself can. Run
`mike service status` to see its process id. `./scripts/deploy-proxy.sh` detects the service and
deploys through it (copy the binary, restart the proxy service), so only a proxy started by hand
with `mike serve` or an older copy of the script runs into this.

### `403` from the inspect API with `{"error":"host not allowed"}` or `{"error":"origin not allowed"}`

There is no authentication on the inspect API. The only thing guarding it is a Host/Origin
allow-list middleware (`crates/mike/src/api/host_guard.rs`), and this 403 is that middleware
rejecting the request before any handler runs. The response body is JSON and names which check
failed.

`host not allowed` means the request's `Host` authority is not one the guard accepts: the
listener's own address and `localhost`, `127.0.0.1` and `[::1]`, each at the listener's actual
port, plus anything in `MIKE_INSPECT_ALLOWED_HOSTS`. It fires when something reaches the API by
a LAN IP, a machine hostname, a container name or a reverse-proxy `Host`, or on the wrong port.
A request with no `Host` header is rejected too (`host required`) unless it is HTTP/2 or
HTTP/3 and carries a URI authority. Other messages name their own cause: `invalid host`,
`conflicting request authority`, `invalid origin`, and `cross-origin browser request requires an
approved origin` for an Origin-less browser request with cross-site Fetch Metadata.

`origin not allowed` means an `Origin` header was present and matched neither a configured SPA
origin nor the request's own `Host` authority (scheme, host and effective port all count).
`Origin: null` — a page opened from `file://`, a sandboxed iframe, some redirect chains — is
always rejected and cannot be allow-listed.

Fix by adding what is legitimately calling in:

- `MIKE_INSPECT_ALLOWED_HOSTS` (config `inspect_allowed_hosts`, default empty) — extra
  authorities the `Host` header may carry; a name without a port uses the listener's port.
- `MIKE_SPA_ORIGINS` (config `spa_origins`) — full origins allowed to make cross-origin browser
  calls. It does not add their hostnames as accepted `Host` values. When the list is empty the
  guard falls back to `http://127.0.0.1:9990` and `http://localhost:9990`.

The bind address of the listener is allow-listed automatically, unless it is a wildcard address.
[`LOCAL_ACCESS.md`](LOCAL_ACCESS.md) has the full rules.

**Allow-listing a host does not add authentication.** It widens who can reach an API that will
serve full captured request and response bodies to anyone who asks. Read
[`SECURITY.md`](SECURITY.md) before opening the bind beyond loopback — in particular *What the
Host/Origin guard does not do*.

### `ERR_MODULE_NOT_FOUND` from `vite dev`

A stale `.svelte-kit` cache. `cd frontend && rm -rf .svelte-kit && pnpm run dev`, then restart.

### MCP tools return connection errors

Check the proxy is up: `curl http://127.0.0.1:9989/api/health`. Confirm `MIKE_INSPECT_URL` in
`~/.claude.json` matches `MIKE_INSPECT_ADDR` (default `http://127.0.0.1:9989` on both sides).
If the bridge was built before a recent tool-shape change, rebuild and reinstall it —
`make install` — and restart Claude Code; a stale `mike-mcp` binary can also surface as a
connection or schema-mismatch error rather than an obvious version mismatch.

### Claude Code request hangs

The proxy forwards rate-limit and `retry-after` headers verbatim — a hang usually means upstream
is rate-limiting. Check the request detail view; 429/529s show status and headers.

### Database latency looks better than the calls felt

`db_query_duration_ms{method, outcome}` records one sample per persistence call that started.
`outcome` is `ok` or `err` only when the call's result arrived. A wait abandoned before then
(its task cancelled, a timeout around it, a panic) records `dropped_before_completion`, timed
from the start to the drop. Read completed latency from `outcome=~"ok|err"` and abandoned waits
separately:

```promql
histogram_quantile(0.95, sum by (le, method) (rate(db_query_duration_ms_bucket{outcome=~"ok|err"}[5m])))
sum by (method) (rate(db_query_duration_ms_count{outcome="dropped_before_completion"}[5m]))
```

A `dropped_before_completion` sample says the caller stopped waiting, not what the database did:
the statement may still have committed. Do not read it as a rollback or as nothing written.

### Frontend shows stale data after a change

The SPA runs under `vite dev` with HMR, so most edits apply live. If a change doesn't show up (or
HMR gets stuck), reload the browser tab first — a full dev-server restart is rarely needed. If
reload doesn't help, HMR occasionally splits a context store across the old and new module
instance, leaving an in-memory guard wedged; a full reload (not just HMR) resolves that.

### `make lint` fails on the rules budget

`make lint` runs `scripts/lint-rust` and then `scripts/rule-checks/rules-budget.sh`, which checks
that the always-loaded `.claude/rules/` set is still under its 10,000-token budget. A rule that
forgets its `paths:` frontmatter fails here. Exit 1 fails lint and the script re-runs itself so
its output is visible; exit 2 (no `.claude/rules/` directory) prints the reason and moves on.

### An LLM worker stops running and nothing says so

Two rows in the capabilities list (shown on the home page and read from `/api/capabilities`) judge
the LLM workers by what they did, not by whether a key is set. `llm_role_health` reads the last
real outcome of each role from `llm.calls` over the last seven days: the steerers, turn judge,
session judge, lesson proposer and lesson relations. It reads `degraded` when the newest call
failed, `inert` when the last three or more calls all failed, and `degraded` when a role whose
switch is on has made no call for 48 hours (72 for the lesson proposer) or at all in seven days.
The fix line depends on the failure: HTTP 402 names the OpenRouter top-up page, 401 and 403 name
the credentials, and a `transport_error` on a `codex/` model names `codex login status` (on a
`claude/` model, `claude auth status`). The row's detail starts with the active `llm.provider`.
When that provider is `claude` or `codex` and its login is missing on the machine that runs the
proxy, the row reads `inert` with the login fix, and no other provider takes over.
`worker_toggles` reads `degraded` when the steerer, turn judge, session judge or `judge_lessons`
switch has been off for two days or more, and for `judge_lessons` it adds how many corrected
operator turns arrived since the switch went off. This is the situation of mike#608, where
`judge_lessons` stayed off for nine days after the OpenRouter account ran out of credit.

Calls made through the machine's Codex or Claude Code login (`codex/<model>`, `claude/<model>`)
carry tokens but no price. The
spend API reports them as `subscription_call_count` on every group, caller and total, and the
usage page shows "Subscription" and "no per-call price" for a row where every call was one. The
weekly learning audit reports `worker_toggle_off` and `llm_role_failing` findings and the number
of days per month on which the lesson proposer made a call.

### Remote Control stops linking channels to sessions

mike depends on Claude Code's Remote Control protocol, which has no public spec and can change in
any client update. The proxy's health status carries a `remote_control_link` row (shown on the home
page and in `/settings`): of the Remote Control channels first seen in the last 24 hours, at least
15 minutes ago, how many were linked to a local session. It turns `degraded` when there are at
least five such channels and fewer than half are linked. That is what a protocol change looks like
in practice: on 2026-09-10 Claude Code 2.1.272 stopped sending the events mike linked channels
from, and no new channel was linked for six days (mike#268). On a normal day 85% to 99% link.

To find what moved, run `make rc-protocol-check` by hand. It runs two scripts (mike#236), neither
of which is part of lint:

- `scripts/rc-protocol-check-binary` — does each installed Claude Code client still carry the
  wire shapes mike's Remote Control support depends on (endpoint paths, frame field names, status
  values), read out of the compiled binary with `strings`.
- `scripts/rc-protocol-check-capture` — the same question asked of the traffic mike captured in
  the last hour, from the `remote_control.calls` and `remote_control.call_payloads` tables.

Each has a `--self-test` that proves it can fail without touching anything real. Exit 1 names the
assertion that moved; exit 2 means there was nothing to check (no binary found, or no Remote
Control traffic in the last hour).
