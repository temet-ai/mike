---
title: Troubleshooting
status: active
created: 2026-06-17
updated: 2026-11-01
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
`MIKE_TEST_ADMIN_URL` and the rest — and the shared build directory, so it compiles into
`./target` and any DB-backed test fails on an unset variable. Use `scripts/cx <cargo args>`
instead: it resolves the repo root from its own location (so it runs from any cwd), sources
`.env`, and exports `CARGO_TARGET_DIR` (default `~/.cargo-target-shared`). Build output lands
there, not in `./target`. `scripts/cx` also reclaims space in that shared directory when free
space is below 30GB before a build, and again after any command that changed `Cargo.lock`.

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

On the development host the capture DB is the container `armitage-pg`, and
`scripts/capture-db/up.sh` starts it — that is what the script's message names when the
unreachable address is on this machine. For an installation-owned local PostgreSQL,
use `mike --config PRIVATE_CONFIG database provision`, or the thin
`scripts/local-db/up.sh --config PRIVATE_CONFIG` development wrapper. The Compose resource
is embedded and only `mike --config PRIVATE_CONFIG serve` runs migrations. See
[`INSTALLED_DATABASE.md`](INSTALLED_DATABASE.md). `docker-compose.dev.yml` in the repo root still defines a
`postgres` service under the `db` profile; there is no root `docker-compose.yml`, so a bare
`docker compose up -d postgres` fails with "no configuration file provided" — pass
`-f docker-compose.dev.yml --profile db`.

If queries fail with `connection refused` while Docker reports the container "healthy", check
host **disk space**: a full disk produces I/O errors that kill the DB port even though the
container looks up. The shared cargo target dir (`CARGO_TARGET_DIR`, typically
`~/.cargo-target-shared`) is the usual culprit. `scripts/cx` runs the cleaner itself when free
space is low, and a scheduled job runs the same cleaner as a backstop; if you need to reclaim
space by hand, clear old artifacts there rather than running `cargo clean`, which discards the
shared cache every session is using and forces a cold rebuild of every test harness.

### Capture DB on omit is unreachable

The shared capture database (`claude_proxy`) runs as the Docker container `armitage-pg`, defined
in `scripts/capture-db/docker-compose.yml`. Every connection string lives in the untracked
`.env`: `MIKE_DATABASE_URL` and `MIKE_TEST_ADMIN_URL`, dialling `127.0.0.1:5440` on the host
itself and the host's LAN address and port from another machine. Nothing tracked in the
repository carries a database URL — not `.cargo/config.toml`, which holds only clippy rustflags
and says outright never to commit a `MIKE_TEST_ADMIN_URL` default. (The root `config.toml` is a
dead file: nothing loads it, and it is unrelated to the per-user `~/.config/mike/config.toml`
that mike does read. Do not treat it as a source of truth.)

The container publishes `5440` on every interface and restarts with the docker daemon
(`restart: unless-stopped`, `docker.service` enabled), so after a reboot the DB is back as soon
as docker is.

#### Symptoms

- DB-backed tests fail in bulk with `Error: pool timed out while waiting for an open connection`
  (a dozen or more failing at once in one `make check` is the signature — not a handful of
  flakes).
- `GET /api/health` reports `persistence: {enabled: true, reachable: false}` and top-level
  `status: degraded`.

A `degraded` health payload whose *only* cause is the `llm` block (OpenRouter paused) is a
separate, currently-normal condition — read `persistence.reachable` specifically to judge
whether capture itself is healthy; don't conflate the two.

#### Diagnose before blaming the app

```bash
docker ps --filter name=armitage-pg    # on omit: is the container up and healthy?
docker logs --tail 50 armitage-pg      # crash recovery in progress, checksum failure, disk full
scripts/proxy-q health                 # from any checkout: dials MIKE_DATABASE_URL and counts rows
```

#### Fix

```bash
scripts/capture-db/up.sh   # idempotent: starts the container if it is down, waits for health
```

Do **not** restart `mike` and do **not** re-run any migration. The running proxy self-heals once
the DB is back: the sqlx pool reconnects and `persistence.reachable` flips back to `true`
within seconds, no restart required.

#### Onboarding another machine to the shared DB

Any machine running its own `mike` proxy can capture into the same `claude_proxy` DB instead of a
local-only one. That is what makes cross-machine session history possible: both machines'
captures land in one place, and either machine's `mike-mcp` (pointed at its own local inspect
bind, default `127.0.0.1:9989`) sees both.

1. **Open the DB port to the LAN on the database host.** The container publishes `5440` on all
   interfaces, but a host firewall that defaults to deny still filters it — a published docker
   port is not automatically routed around the host's INPUT chain. Add a rule scoped to the LAN
   subnet for TCP `5440`. Nothing is needed inside the container: its `pg_hba.conf` already
   allows `host all all all scram-sha-256`.
2. **On the other machine**, set `MIKE_DATABASE_URL` in its own untracked `.env` to the database
   host's LAN address and credentials, instead of whatever local/fallback DB it was using. Do not
   commit that URL anywhere.
3. **Restart its proxy stack** (`./scripts/start-proxy.sh`) so the running `mike` process picks up
   the new URL. From then on, everything that machine captures on its own `:9988`/`:9989` writes
   into this shared DB — no change needed to its `mike-mcp` config, since that still talks to its
   own local inspect bind, which now simply reads from the shared DB.

If the other machine cannot reach the shared Postgres at all (not just slow or misconfigured), it
should use [local DB fallback](LOCAL_DB_FALLBACK.md) — capture to a local Postgres and drain
those rows into the shared DB on the next start that reaches it — rather than blocking on network
access.

#### History: the WSL2 portproxy era

**Historical only — this topology no longer exists.** The same container once ran inside a WSL2
Ubuntu distro on the Windows install of the same hardware, reachable from the LAN only through an
elevated `netsh portproxy` rule that pinned the distro's IP, so every WSL restart broke the link
until `scripts/migrate-db-to-armitage.sh lan-fix` rewrote it. When the machine moved to Arch
Linux the cluster was copied byte-for-byte out of the distro's `ext4.vhdx` into the docker volume
`armitage_pg_data`. The `lan-check`, `lan-fix` and `tunnel` subcommands remain in that script for
the record; they target a topology that is gone, and nothing current should call them.

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

### `403` from the inspect API with `{"error":"host not allowed"}` or `{"error":"origin not allowed"}`

There is no authentication on the inspect API. The only thing guarding it is a Host/Origin
allow-list middleware (`crates/mike/src/api/host_guard.rs`), and this 403 is that middleware
rejecting the request before any handler runs. The response body is JSON and names which check
failed.

`host not allowed` means the request's `Host` header resolved to a host that is not
allow-listed. The guard always allows `localhost` and any loopback IP, so this only fires when
something is reaching the API by a non-loopback name or address: a LAN IP, a machine hostname, a
container/service name, a reverse-proxy `Host`. A request with no `Host` header at all is
allowed through.

`origin not allowed` means an `Origin` header was present and matched neither a configured SPA
origin nor the request's own `Host` authority. `Origin: null` — a page opened from `file://`, a
sandboxed iframe, some redirect chains — is always rejected and cannot be allow-listed.

Fix by adding what is legitimately calling in:

- `MIKE_INSPECT_ALLOWED_HOSTS` (config `inspect_allowed_hosts`, default empty) — extra hostnames
  the `Host` header may carry.
- `MIKE_SPA_ORIGINS` (config `spa_origins`) — full origins allowed to make cross-origin browser
  calls. Their hostnames are also added to the allowed-host set. When the list is empty the guard
  falls back to `http://127.0.0.1:9990` and `http://localhost:9990` and logs a warning.

The hosts of `inspect_listen_addr` and `spa_bind` are allow-listed automatically, unless they
bind a wildcard address.

**Allow-listing a host does not add authentication.** It widens who can reach an API that will
serve full captured request and response bodies to anyone who asks. Read
[`SECURITY.md`](SECURITY.md) before opening the bind beyond loopback — in particular *What the
Host/Origin guard does not do*.

### History: `403` hitting `/api/compose`

**Historical only.** The compose sandbox (`POST /api/compose`, the `MIKE_ALLOW_COMPOSE` toggle,
and the `claude -p` subprocess spawn) was removed from the codebase in commit `db533f43` — there
is no `/api/compose` route, no `allow_compose` config field, and no `compose` MCP tool. If older
documentation, a bookmark, or a stale build artifact references `MIKE_ALLOW_COMPOSE`, that
reference is stale: the feature does not exist and there is nothing to re-enable.

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

If `rc-protocol-check-capture` reports the capture database as unreachable while running from a
linked git worktree (anything under `.claude/worktrees/`, not the main checkout), that specific
diagnosis is almost always wrong: `scripts/proxy-q` reads the database URL from an untracked
`.env` file that only the main checkout has, so the script resolves the main checkout via
`git rev-parse --path-format=absolute --git-common-dir` before shelling out to `proxy-q`, and a
genuinely unreachable database still reports as unreachable from there. If you see "unreachable"
from a worktree despite the main checkout working fine, that resolution step itself has broken —
check that `scripts/proxy-q` still exists at the main checkout's root.
