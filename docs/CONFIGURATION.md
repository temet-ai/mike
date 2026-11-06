---
title: Configuration
status: active
created: 2026-07-08
updated: 2026-11-06
summary: Boot configuration reference — the four-layer precedence including the per-user config file, how MIKE_* environment variables map to fields, every documented knob and its real default, the variables that are special-cased or ignored, and persistent storage setup.
---

# Configuration

This is the boot configuration reference for `mike`. It is for anyone going beyond the default
quick start: choosing your own ports, pointing at a Postgres you already run, running the
in-memory mode, running the DB-backed integration tests, or launching the processes by hand.

If you haven't stood up the proxy yet, start with [`GETTING_STARTED.md`](GETTING_STARTED.md).
Runtime feature toggles and their defaults are in [`FEATURES.md`](FEATURES.md); the trust
boundary is in [`SECURITY.md`](SECURITY.md).

## Boot config precedence

mike loads its boot configuration from four layers, low to high precedence:

1. Built-in defaults (`ProxyConfig::default()`)
2. The per-user config file, `config.toml`, described in the next section
3. `MIKE_*` environment variables
4. CLI flags

Later layers override earlier ones. `--config <path>` selects the TOML file for this
installation. The installed binary does not search the current directory or its ancestors
for `.env`. Import an existing file explicitly with `mike import-dotenv <path>`;
repository development scripts still load their explicitly selected checkout environment.
The import preserves the source file and refuses unsupported keys unless
`--ignore-unknown` is given.

### The per-user config file

The file is `$XDG_CONFIG_HOME/mike/config.toml`, or `~/.config/mike/config.toml` when
`XDG_CONFIG_HOME` is unset or not an absolute path. Linux and macOS use the same rule, so macOS
does not use `~/Library/Application Support` for it. `mike config path` prints the path the
binary uses, and `mike config path --state` prints the matching state directory,
`$XDG_STATE_HOME/mike` or `~/.local/state/mike`, which also holds the machine identity and the
launch queue. The location is fixed at startup from the environment the command was started
with. An explicitly selected nondefault file gets separate private data, state and log
directories beside it; `mike --config <path> config path --state` prints that state path.

Every key is the name of a `MIKE_*` variable without the `MIKE_` prefix, in lower case:

| In the config file | Same as the environment variable |
|---|---|
| `database_url = "postgres://…"` | `MIKE_DATABASE_URL` |
| `listen_addr = "127.0.0.1:9988"` | `MIKE_LISTEN_ADDR` |
| `persist_enabled = false` | `MIKE_PERSIST_ENABLED` |
| `launch_backend = "herdr"` | `MIKE_LAUNCH_BACKEND` |
| `steerer_openrouter_api_key = "sk-or-…"` | `MIKE_STEERER_OPENROUTER_API_KEY` |
| `[embedding]` table with `url = "http://…"`, or `embedding__url = "http://…"` | `MIKE_EMBEDDING__URL` |

Setup and `mike claude-code` also keep a few settings of their own in this file. They are read directly from the selected file; boot settings continue to follow the
precedence above:

| Key | Holds |
|---|---|
| `setup` | `unfinished` while the setup wizard has not reached its last step, then `done`. A bare `mike` on a terminal opens the wizard while it says `unfinished`. |
| `claude_route` | `sidecar` or `plain`: how Claude Code reaches mike (`mike claude-code route`). |
| `claude_function` | The name of the shell function that `mike claude-code shell` wrote. |
| `claude_mods` | The mod folders that `mike claude-code mods` added to `CLAUDE_CODE_PLUGIN_DIRS`, so that `mike uninstall` removes exactly those. |
| `launch_auto_trust` | `true` lets `mike claude` write the folder-trust answer into `~/.claude.json`. Without this key the value is `false`; the wizard saves `false` unless you choose otherwise. |
| `setup_features` | A feature choice (`watch-only`, `recommended` or `everything`) saved while the proxy was not running. The proxy applies it once at its next start and removes the key. |

The mapping rules are the ones in [How `MIKE_*` names map to fields](#how-mike_-names-map-to-fields).
A key written with the `MIKE_` prefix is rejected with a message that says to drop it, so a typo
cannot be ignored silently. Strings, numbers and booleans are accepted. An array becomes the
comma-separated text the variable would hold (`spa_origins`), or space-separated for
`launch_claude_args`. An empty string means the setting is not set.

Values are literal TOML. Nothing is interpolated, so a `$`, a quote or a backslash in a password is
stored and read back as written. This differs from a shell-sourced `.env`, where `$` starts an
expansion.

A key ending in `_from_env` holds the name of an environment variable instead of a value:
`steerer_openrouter_api_key_from_env = "OPENROUTER_API_KEY"` sets
the effective provider key from `OPENROUTER_API_KEY` when that variable is set, without changing the process environment. A literal
`steerer_openrouter_api_key` wins over the named variable. Setup stores the name when you type one,
so no secret is written to disk.

Setup's storage, provider and terminal writes use the selected private configuration. They
lock the writer, validate the resulting boot settings, keep a private backup, and replace
the file atomically. Existing directories must be owner-only; symlinks and multiply linked
files are refused. A writable file owned by the user is replaced with mode 0600. These transactions reserialize TOML. The older `mike config import-env`
command retains comments and key order through the user-config editor.

Commands that load boot configuration refuse invalid TOML or unsupported settings. Errors
omit configuration values. `--version`, `--help` and `--build-info` do not load configuration.

To move a checkout's `.env` into the file, run `mike config import-env [path]` (default
`./.env`). It copies the `MIKE_*` assignments, reading values without any interpolation, and
keeps every key the config file already has. Running it again changes nothing.

### Sandbox mode

With `MIKE_SANDBOX=1` in the environment, `mike doctor` and the setup wizard run no `docker`,
`brew`, `claude` or `codex` command, start no terminal-backend probe and open no browser.
`doctor` then lists `claude` without reading its version. The repository's `.cargo/config.toml`
sets the variable for builds, runs and tests started through cargo; set `MIKE_SANDBOX=0` in the
shell to probe for real from a checkout.

Commands use the selected configuration and their explicit environment regardless of the
working directory. They do not discover another serving checkout or inherit its `.env`.

This is the **boot-time** config only. The live source of truth for runtime tuning — feature
toggles, steerer parameters, weasel words, reminders, recall, filter rules — is `/settings`,
which reads and writes the database. A stored setting overrides the boot value.

### Inspecting the effective config

```bash
mike --version               # version and git commit; needs no database and no config
mike doctor [--json]         # read-only install check: config, database, ports, sidecar, claude, MCP
mike print-config            # boot config as TOML, then the LIVE OVERLAY stored in Postgres
mike print-config --shell    # a curated subset as KEY=VALUE shell exports
mike migrate-check           # read-only pre-migration check against the target DB
```

`mike doctor` makes no change and calls no model. Each check reports `ok`, `warn` or `FAIL`
with one line of detail, and a failing or warning check names the `mike setup <step>` that
fixes it (`database`, `service`, `claude`, `models`, `embeddings`, `terminal` or `features`).
It checks the config file, the binary version, the database (URL, connection, PostgreSQL 19, the `vector`, `pgcrypto` and `pg_trgm` extensions, migrations), the proxy, inspect and web
UI ports and whether the running proxy is this build, the web UI embedded in the binary, the
forward sidecar and its certificate, `claude` and its version, `mike-mcp` and its registration in
`~/.claude.json` (the registered command must be a file that exists), the route Claude Code takes
to mike and the shell function, the available background-model providers (by variable name, never the key), the
embedding server and the terminal backend. It exits 0 when nothing failed and 1 otherwise;
`--json` instead prints the stable installation readiness report with eight allowlisted checks.
`--support-bundle` prints that redacted JSON report to standard output for local review.
`mike spawn doctor` is separate: it validates one terminal backend live.

`print-config` starts with comment lines that name the config file it read (or say it does not
exist) and list the explicit `MIKE_*` process environment variables. File settings are merged
directly, without exporting them into the process environment. Values are not printed in those comments. It then resolves all
four layers and prints the result as the boot config. It renders TOML as an *output*
format only — nothing reads that TOML back, and the database URLs in it are password-redacted.
After the boot config it prints a `LIVE OVERLAY` section: the rows of `config.settings` and
`config.feature_toggles`, which override the boot values at runtime. That section, not the boot
block, shows what a running session got (for example `embedding.enabled` and `composition_enabled`
can be `false` at boot and `true` live). Secret-looking keys are printed as `<redacted>`. When the
database cannot be reached the section reads `unavailable (<reason>)` and the output is boot config only.

`print-config --shell` emits a deliberately curated subset for `eval`/sourcing (network, store
capacity, observability, persistence, retention, frontend, body budgets, mailbox). It omits
`MIKE_DATABASE_URL`, `MIKE_LOCAL_DATABASE_URL` and every `MIKE_EMBEDDING*` variable by
design. `start-proxy.sh` uses it so the launcher, `vite dev` and the MCP bridge agree on ports.

`migrate-check` connects to `database_url` and compares the binary's embedded `sqlx` migration
checksums against the `_sqlx_migrations` table already applied there. It exits non-zero on a
checksum mismatch or when the binary is older than what the database already has. It applies
nothing.

## How `MIKE_*` names map to fields

The rule is mechanical: strip the `MIKE_` prefix, lowercase the rest, and that is the field
name. A double underscore (`__`) descends into a nested config struct — `MIKE_STEERER__MODEL`
sets `steerer.model`.

Two names are remapped for historical reasons: `MIKE_LISTEN_ADDR` → `proxy_listen_addr` and
`MIKE_INSPECT_ADDR` → `inspect_listen_addr`. These are the canonical spellings and the ones
`print-config --shell` emits.

Two consequences worth knowing:

- **Booleans are permissive.** `1`, `0`, `true`, `false`, `yes`, `no` and similar are all
  accepted. An unparseable value is a boot failure, not a silent default.
- **Most nested structs reject unknown fields.** A typo such as
  `MIKE_STEERER__JUDG__ENABLED` fails the boot rather than being ignored. Unknown *top-level*
  keys are ignored.

### Special-cased variables

Three variables are comma- or whitespace-split into lists by hand, because the environment
layer can only produce scalars:

| Variable | Split on | Sets |
|---|---|---|
| `MIKE_SPA_ORIGINS` | `,` | `spa_origins` |
| `MIKE_INSPECT_ALLOWED_HOSTS` | `,` | `inspect_allowed_hosts` |
| `MIKE_LAUNCH_CLAUDE_ARGS` | whitespace | `launch_claude_args` |

Setting `MIKE_SPA_ORIGINS` **replaces** the whole default list rather than extending it — if
you add one origin, include the two localhost defaults too.

### Ignored variables

**The entire `embedding` sub-tree is ignored at boot.** The loader resets `config.embedding` to
its defaults after the environment layer has been applied, so `MIKE_EMBEDDING__ENABLED`,
`MIKE_EMBEDDING__URL`, `MIKE_EMBEDDING__MODEL` and `MIKE_EMBEDDING__CHUNK_MAX_CHARS` are all
silently discarded.

The only boot-time embedding control that works is the CLI flag `--embedding <bool>`, and it
sets `enabled` only. `url`, `model` and `chunk_max_chars` are not settable at boot at all —
change them in `/settings`, which is where the runtime values come from.

### The OpenRouter API key

The key is never stored in the database or set through `/settings`. At boot it comes from the
config file (`steerer_openrouter_api_key`, or the variable named by
`steerer_openrouter_api_key_from_env`), and an environment variable overrides that:

1. `MIKE_STEERER_OPENROUTER_API_KEY` — note the **single** underscore before `OPENROUTER`
2. `OPENROUTER_API_KEY`

A running proxy re-reads the key from the variable named by the `steerer.openrouter_api_key_env`
setting. It is never written out by `print-config`. With no key set, a job whose model runs on OpenRouter
is inert regardless of its toggle, and a job whose model runs on the local `claude` or `codex`
login still runs; see
[Choosing the provider and the model for each job](#choosing-the-provider-and-the-model-for-each-job)
and [`SECURITY.md`](SECURITY.md).

### Choosing the provider and the model for each job

Every background job that calls a language model (steerers and their sub-judges, the turn judge,
the session judge, the lesson proposer, lesson relations, the turn guard labeller) runs on the
provider and the model its model setting names. The setting holds one string that carries both
choices:

| Stored value | Provider | Model sent |
|---|---|---|
| `claude/<model>`, for example `claude/sonnet` | Claude (the local `claude` login) | `<model>` |
| `codex/<model>`, for example `codex/gpt-6-sol` | Codex (the local `codex` login) | `<model>` |
| `openrouter/<vendor>/<model>`, for example `openrouter/anthropic/claude-sonnet-5-5` or `openrouter/openai/gpt-6-sol` | OpenRouter | `<vendor>/<model>` (the `openrouter/` prefix is removed before the HTTP call) |
| any other name, for example `google/gemini-3.8-flash` | OpenRouter | the name unchanged |

The last row keeps every value stored before the explicit prefix existed working. OpenRouter also
serves Claude and GPT models, so `openrouter/anthropic/claude-sonnet-5-5` and
`anthropic/claude-sonnet-5-5` both go to OpenRouter and use the OpenRouter key; only a `claude/` or
`codex/` prefix selects a local login. An OpenRouter model id that itself begins with
`openrouter/`, such as `openrouter/auto`, is still an OpenRouter model id; write it as
`openrouter/openrouter/auto` to make the provider explicit.

The settings that take this value are `steerer.model`, `judge_worker.model`, `session_judge.model`,
`judge_lessons.model`, `lesson_relations.proposer_model`, `lesson_relations.adjudicator_model`, the
nine `llm.provider_models.<provider>.<tier>` settings and the model of each steerer (create, update
and the bulk "set all steerer models" call). Each accepts any of the four forms above. A blank value
means "inherit", except for the nine provider model settings, which must name a model. A provider
prefix with no model after it (`claude/`, `codex/`, `openrouter/`) is refused. A
`llm.provider_models.<provider>.<tier>` setting must be written in its own provider's form. The
bulk steerer call additionally checks an OpenRouter model against the OpenRouter catalogue (it must
be a known model that supports structured output); a `claude/` or `codex/` model is accepted
without a list check because the two CLIs publish no authoritative list.

A job needs the OpenRouter key only when its own model runs on OpenRouter. A job on a `claude/` or
`codex/` model starts and runs with no key at all, and a missing key does not stop any other job. A
job whose provider is not available on the machine (no key for OpenRouter, no installed or
logged-in `claude` or `codex`) is reported `inert` by the `steering` and `llm_role_health` rows of
`/api/capabilities`, and the row names the jobs affected and what is missing; the other jobs are
not affected. A call that reaches an unavailable provider is refused before anything is sent, and
nothing is recorded in `llm.calls`.

Evaluation runs (`mike eval run`, `mike guard replay`, `mike verifier-replay` and the session-end
shadow worker) keep the model they were asked to test, whatever `llm.provider` says. When that
model is an OpenRouter model and there is no OpenRouter key, they refuse with a message that names
the model and the provider setting instead of calling OpenRouter with an empty key.

`GET /api/llm/providers` answers which providers this machine can use and what each offers:

```json
{
  "llm_provider": "per_role",
  "providers": [
    {
      "id": "openrouter",
      "label": "OpenRouter",
      "prefix": "openrouter/",
      "available": true,
      "detail": "An OpenRouter API key is configured; it is read from the environment variable MIKE_STEERER_OPENROUTER_API_KEY",
      "fix": "",
      "key_env_var": "MIKE_STEERER_OPENROUTER_API_KEY",
      "models_authoritative": true,
      "models": [
        { "id": "anthropic/claude-sonnet-5-5", "ref": "openrouter/anthropic/claude-sonnet-5-5",
          "label": "Claude Sonnet 5.5", "structured_output": true, "known_good": false }
      ],
      "models_error": null
    },
    { "id": "claude", "label": "Claude", "prefix": "claude/", "available": false,
      "detail": "...", "fix": "...", "key_env_var": null, "models_authoritative": false,
      "models": [ { "id": "haiku", "ref": "claude/haiku", "label": "haiku" } ], "models_error": null },
    { "id": "codex", "label": "Codex", "prefix": "codex/", "available": true,
      "detail": "Logged in using ChatGPT", "fix": "", "key_env_var": null, "models_authoritative": false,
      "models": [ { "id": "gpt-6-sol", "ref": "codex/gpt-6-sol", "label": "gpt-6-sol" } ], "models_error": null }
  ]
}
```

`available` for `claude` and `codex` comes from `claude auth status` and `codex login status`
(neither calls a model; the results are cached for five seconds), using `MIKE_CLAUDE_CLI_BIN` or
`claude` on the proxy's `PATH` for Claude and `codex` on the `PATH` for Codex. For OpenRouter it is
true when a key is present; the response gives the name of the environment variable that supplies
it and never the key. `ref` is the string to store in a model setting to pick that model. The
OpenRouter list is the model catalogue (`models_authoritative` is true, and `models_error` explains
an empty list when the catalogue could not be fetched). The Claude list is the aliases `haiku`,
`sonnet` and `opus` plus every Claude model named in a setting, and the Codex list is the Codex
tier defaults plus every Codex model named in a setting. The route is not exposed as an MCP tool.

### Running every worker from one provider

The setting `llm.provider` (at `/settings`, group "LLM provider") sends every backend LLM role to
one provider. Its values are `per_role` (the default: each role keeps the model in its own setting,
exactly as before), `openrouter`, `claude` and `codex`. With any value but `per_role`, a role's own
model setting is ignored and the role uses the provider's model for its tier instead. The roles
fall into three tiers:

| Tier | Roles |
|---|---|
| fast | session judge, lesson proposer, lesson-relations adjudicator |
| standard | turn judge |
| strong | steerers and their sub-judges, turn guard labeller, lesson-relations proposer |

Each provider has one model per tier, set in nine settings named
`llm.provider_models.<provider>.<tier>`. The defaults are `google/gemini-3.5-flash-lite`,
`google/gemini-3.8-flash` and `openai/gpt-5.6-luna` for OpenRouter; `claude/haiku`, `claude/sonnet`
and `claude/opus` for Claude; `codex/gpt-6-luna`, `codex/gpt-6-sol` and `codex/gpt-6.1-sol` for
Codex. A Claude or Codex model must start with `claude/` or `codex/`, and an OpenRouter model must
not (an explicit `openrouter/<vendor>/<model>` name is accepted for OpenRouter). The lesson-relations proposer (strong) and adjudicator (fast) must stay on different models.

A change to the provider or to a tier model applies on the next cycle and moves the live turn judge
and session judge bundles to the new models, the way a change to `judge_worker.model` does; the
settings response lists the swaps in `live_bundle_swaps`. Evaluation runs (`mike eval`, the
session-end shadow worker) keep the model they were asked to test whatever the provider says.

`claude` and `codex` use the login already on the machine, so no OpenRouter key is needed for the
workers to start. If the chosen provider's login or key is missing, the `llm_role_health` row on
the home page names the provider and reads `inert` with the fix (`claude auth status`,
`codex login status`, or the OpenRouter key); mike does not fall back to another provider.

### Running a worker on Claude instead of OpenRouter

A model setting can also be `claude/<model>` (for example `claude/sonnet`, or a full model name).
mike then runs the local `claude` binary in print mode (`claude -p --output-format json
--json-schema <verdict schema> --no-session-persistence --tools "" --setting-sources ""`, with the
system prompt in a temporary file and the user prompt on stdin) under the Claude Code login already
on the machine, and reads `structured_output` back as the verdict. The child process gets only
`PATH`, `HOME`, `USER`, `LANG`, `LC_ALL` and `TMPDIR`, so `ANTHROPIC_BASE_URL` and the proxy
variables never reach it and the call does not return through mike's own proxy. The binary is
`claude` on the proxy's `PATH`, or the path in `MIKE_CLAUDE_CLI_BIN`. Reasoning effort comes from
`llm.reasoning_effort` (`low` to `max` become `--effort`; `none` omits it, and a model that
rejects `--effort` is resent without it). The call is recorded in `llm.calls` with transport
`claude`, the session id as `generation_id` and the token counts Claude reports; `cost_usd` is
null because the call runs on the subscription, so the daily spend ceiling does not count it.

### Running a worker on Codex instead of OpenRouter

Any model setting that names an OpenRouter model (`steerer.model`, `judge_worker.model`,
`session_judge.model`, `judge_lessons.model`, `lesson_relations.proposer_model`,
`lesson_relations.adjudicator_model`) also accepts `codex/<model>`, for example
`codex/gpt-6-sol`. A call on such a model does not go to OpenRouter: mike runs the local
`codex` binary (`codex exec`, ephemeral, read-only sandbox, user config and rules ignored,
the verdict schema passed as `--output-schema`) under the ChatGPT login already on the
machine, and reads the final message back as the verdict. Reasoning effort comes from
`llm.reasoning_effort` (`none` becomes Codex's `minimal`). The call is recorded in
`llm.calls` like any other, with the thread id as `generation_id` and the token counts
Codex reports. Codex reports no price, so `cost_usd` is null and the daily spend ceiling
does not count these calls; the call-count and per-minute ceilings still apply. The
proxy unit's `PATH` must reach `codex` (the systemd template carries the mise install
path). A worker whose model is `codex/` starts and runs without an OpenRouter key, whatever
`llm.provider` is, and the key is not sent anywhere on a Codex call.

## Knob table

Defaults are the compiled ones. Every `MIKE_*` name below is derived by the rule above.

The runtime setting `steerer.openrouter_api_key_env` (default `MIKE_STEERER_OPENROUTER_API_KEY`, applies on restart) names the environment variable the key is read from; `/settings` has a Verify button that confirms the variable is present and non-empty.

### Network and binds

| Field | Env var | Default | What it does |
|---|---|---|---|
| `proxy_listen_addr` | `MIKE_LISTEN_ADDR` | `127.0.0.1:9988` | The proxy bind — the address Claude Code points at. Also `--proxy-listen-addr`. |
| `inspect_listen_addr` | `MIKE_INSPECT_ADDR` | `127.0.0.1:9989` | The inspect bind (REST + WebSocket). Also `--inspect-listen-addr`. |
| `spa_enabled` | `MIKE_SPA_ENABLED` | `true` | Whether mike serves its embedded SPA. `scripts/start-proxy.sh` sets this to `0` so `vite dev` can own the port in development. Also `--spa-enabled`. |
| `spa_bind` | `MIKE_SPA_BIND` | `127.0.0.1:9990` | Where the embedded SPA listens. **This bind also serves the whole `/api/*` surface.** Also `--spa-bind`. |
| `frontend_port` | `MIKE_FRONTEND_PORT` | `9990` | Read by `vite dev` and the launcher scripts. It does **not** move mike's own SPA bind — use `spa_bind` for that. Also `--frontend-port`. |
| `spa_origins` | `MIKE_SPA_ORIGINS` | `["http://127.0.0.1:9990", "http://localhost:9990"]` | CORS allow-list for the inspect API, and one of the inputs to the Host/Origin guard. Comma-separated; replaces the default list. |
| `inspect_allowed_hosts` | `MIKE_INSPECT_ALLOWED_HOSTS` | empty | Extra `Host` header values the guard accepts beyond localhost, loopback, the bind addresses and the origin hosts. **This is not authentication** — see [`SECURITY.md`](SECURITY.md). |
| `upstream_url` | `MIKE_UPSTREAM_URL` | `https://api.anthropic.com` | Where the proxy forwards. You should not need to change this. Also `--upstream-url`. |
| `finite_url` | `MIKE_FINITE_URL` | `http://127.0.0.1:3000` | Base URL for the optional fleet-graph enrichment service. |

### Storage

| Field | Env var | Default | What it does |
|---|---|---|---|
| `persist_enabled` | `MIKE_PERSIST_ENABLED` | `true` | Durable Postgres storage. `false` runs fully in memory; see [Option B](#option-b--skip-persistence) below. Also `--persist-enabled`. |
| `database_url` | `MIKE_DATABASE_URL` | unset | Primary Postgres URL, required when persistence is on. Also `--database-url`. |
| `local_database_url` | `MIKE_LOCAL_DATABASE_URL` | unset | Fallback Postgres URL — mike falls over to this when the primary is unreachable at boot. See [`LOCAL_DB_FALLBACK.md`](LOCAL_DB_FALLBACK.md). Also `--local-database-url`. |
| `db_fallback_enabled` | `MIKE_DB_FALLBACK_ENABLED` | `true` | Whether that failover is active. Also `--db-fallback-enabled`. |
| `db_resync_enabled` | `MIKE_DB_RESYNC_ENABLED` | `true` | Whether rows written to the fallback are drained back to the primary. Also `--db-resync-enabled`. |
| `db_connect_timeout_ms` | `MIKE_DB_CONNECT_TIMEOUT_MS` | `10000` | How long `serve` waits for the first connection to the database at boot before it fails or falls back. |
| `db_pool_capture_size` · `db_pool_capture_acquire_timeout_ms` · `db_pool_capture_statement_timeout_ms` | `MIKE_DB_POOL_CAPTURE_SIZE` · `…_ACQUIRE_TIMEOUT_MS` · `…_STATEMENT_TIMEOUT_MS` | `4` · `5000` · `5000` | Connection pool for the proxy's capture writes (request rows, Remote Control call records). Background work never takes these connections. Applies on restart. |
| `db_pool_interactive_size` · `db_pool_interactive_acquire_timeout_ms` · `db_pool_interactive_statement_timeout_ms` | `MIKE_DB_POOL_INTERACTIVE_SIZE` · `…_ACQUIRE_TIMEOUT_MS` · `…_STATEMENT_TIMEOUT_MS` | `6` · `3000` · `15000` | Connection pool for `/api/*` handlers, so every `mike-mcp` read and the web UI. Applies on restart. |
| `db_pool_background_size` · `db_pool_background_acquire_timeout_ms` · `db_pool_background_statement_timeout_ms` | `MIKE_DB_POOL_BACKGROUND_SIZE` · `…_ACQUIRE_TIMEOUT_MS` · `…_STATEMENT_TIMEOUT_MS` | `4` · `30000` · `30000` | Connection pool for every background worker, startup task, the LLM usage recorder and long backfill transactions. Each machine running `serve` opens up to the sum of the three sizes (14 by default); keep the total across machines under Postgres `max_connections`. Applies on restart. |
| `max_requests` | `MIKE_MAX_REQUESTS` | `1000` | Size of the bounded in-memory staging pool. Also `--max-requests`. |
| `max_age` | `MIKE_MAX_AGE` | `24h` | Age bound on that same pool. Also `--max-age`. |
| `max_store_bytes` | `MIKE_MAX_STORE_BYTES` | `3 GiB` | Byte bound on that pool. Also `--max-store-bytes`. |
| `default_ttl` | `MIKE_DEFAULT_TTL` | `365d` | Session retention. The `retention_sweep` worker is on by default and periodically deletes sessions older than this. Also `--default-ttl`. |
| `sweep_at_startup` | `MIKE_SWEEP_AT_STARTUP` | `false` | Delete TTL-expired sessions at boot. Off by recorded operator decision — it is destructive. Also `--sweep-at-startup`. |
| `data_dir` | `MIKE_DATA_DIR` | platform default | Fallback directory for the daemon's log file and pidfile when `log_dir` is unset. It is **not** a blob store; bodies live in Postgres. Also `--data-dir`. |

Migrations run on their own connection, separate from the request-path pool above, so a
migration that needs a lock on a hot table (for example a new foreign key referencing
`requests.requests` or `sessions.sessions`) is not bound by the request path's 5s
`lock_timeout`. `MIKE_MIGRATION_LOCK_TIMEOUT_SECS` (default `180`) controls how long that
migration connection waits for a lock; it is read directly from the environment at connect
time rather than through the boot loader above, so it has no CLI flag and does not
appear in `print-config`. The migration connection also disables `statement_timeout` entirely
— it must be allowed to finish, not merely wait patiently for its lock. A migration that alters a
table capture writes continuously sets its own short `lock_timeout` (`SET LOCAL lock_timeout =
'5s'`), so that it fails and is retried rather than stalling capture.

### Limits

| Field | Env var | Default | What it does |
|---|---|---|---|
| `max_request_body_bytes` | `MIKE_MAX_REQUEST_BODY_BYTES` | `16 MiB` | Request body limit on the proxy bind. Also `--max-request-body-bytes`. |
| `max_response_body_bytes` | `MIKE_MAX_RESPONSE_BODY_BYTES` | `32 MiB` | Response body limit. Also `--max-response-body-bytes`. |
| `max_in_flight` | `MIKE_MAX_IN_FLIGHT` | `500` | Concurrent in-flight requests. Also `--max-in-flight`. |
| `cap_manager.rss_ceiling_bytes` | `MIKE_CAP_MANAGER__RSS_CEILING_BYTES` | `0` | `0` means observe only, never shed. Above zero, an *unmutated* request may be forwarded without being captured while memory is over the high watermark; a request the pipeline rewrote is captured anyway. |
| `cap_manager.interval` | `MIKE_CAP_MANAGER__INTERVAL` | `1s` | A humantime duration string, not a number. |
| `cap_manager.rss_high_watermark_pct` / `_low_watermark_pct` | `MIKE_CAP_MANAGER__RSS_HIGH_WATERMARK_PCT` / `..._LOW_...` | `90` / `75` | Shedding hysteresis. |

### Observability

| Field | Env var | Default | What it does |
|---|---|---|---|
| `otel_endpoint` | `MIKE_OTEL_ENDPOINT` | unset | Enables OTLP trace export. **Spans leave the machine when this is set.** Also `--otel-endpoint`. |
| `log_format` | `MIKE_LOG_FORMAT` | `pretty` | `pretty` or `json`. Also `--log-format`. |
| `log_level` | `MIKE_LOG_LEVEL` | `info` | Seeds `RUST_LOG` when that is unset. Also `--log-level`. |
| `log_dir` | `MIKE_LOG_DIR` | unset | Where rolling log files are written. |
| `log_max_files` | `MIKE_LOG_MAX_FILES` | `7` | How many rolled log files to keep. |
| `RUST_LOG` | env only | — | Standard `tracing_subscriber` filter; overrides `log_level`. |

### Mailbox

| Field | Env var | Default |
|---|---|---|
| `mailbox_stale_minutes` | `MIKE_MAILBOX_STALE_MINUTES` | `10` |
| `mailbox_redelivery_max_attempts` | `MIKE_MAILBOX_REDELIVERY_MAX_ATTEMPTS` | `5` |
| `mailbox_redelivery_backoff_base_secs` | `MIKE_MAILBOX_REDELIVERY_BACKOFF_BASE_SECS` | `180` |
| `mailbox_redelivery_backoff_cap_secs` | `MIKE_MAILBOX_REDELIVERY_BACKOFF_CAP_SECS` | `900` |
| `mailbox_delivery_ttl_minutes` | `MIKE_MAILBOX_DELIVERY_TTL_MINUTES` | `60` |
| `mailbox_max_deliveries_per_turn` | `MIKE_MAILBOX_MAX_DELIVERIES_PER_TURN` | `5` |
| `mailbox_reject_send_to_ended_session` | `MIKE_MAILBOX_REJECT_SEND_TO_ENDED_SESSION` | `true` |
| `workspace_reap_interval_secs` | `MIKE_WORKSPACE_REAP_INTERVAL_SECS` | `120` |

### Launch

`LaunchConfig` is flattened, so every launch key is a top-level flat name with a **single**
underscore: `MIKE_LAUNCH_BACKEND`, not `MIKE_LAUNCH__BACKEND`. Each has a matching
`--launch-*` CLI flag. See [`LAUNCH_DOMAIN.md`](LAUNCH_DOMAIN.md).

| Field | Env var | Default | What it does |
|---|---|---|---|
| `launch_backend` | `MIKE_LAUNCH_BACKEND` | `none` | The terminal backend `mike spawn` opens surfaces in. Accepted values: `herdr`, `herdr-core`, `tmux`, `terminal` (Hyprland only), `ghostty` and `none`; check `mike spawn doctor` for which ones work on this machine. `none` means "not chosen yet" and every launch verb refuses until you pick one. |
| `launch_backend_follow_caller` | `MIKE_LAUNCH_BACKEND_FOLLOW_CALLER` | `true` | Open a launch in the terminal the caller runs in (herdr, tmux) before falling back to `launch_backend`. `false` always uses `launch_backend`. |
| `launch_use_sidecar` | `MIKE_LAUNCH_USE_SIDECAR` | `false` | When on, `mike claude` routes the session through the sidecar as `HTTPS_PROXY`; off, it points the session straight at this proxy. A route chosen with `mike claude-code route` wins over this setting. |
| `launch_proxy_forward_url` | `MIKE_LAUNCH_PROXY_FORWARD_URL` | `http://127.0.0.1:9987` | The sidecar a launched session gets as `HTTPS_PROXY`. |
| `launch_no_proxy` | `MIKE_LAUNCH_NO_PROXY` | `localhost,127.0.0.1` | `NO_PROXY` for a launched session. |
| `launch_ca_cert` | `MIKE_LAUNCH_CA_CERT` | unset | The sidecar CA for `NODE_EXTRA_CA_CERTS`. Unset resolves at use to the first existing of `~/.local/share/proxy-forward/ca-cert.pem` and `<cwd>/.deploy/proxy-forward-ca.pem`. |
| `launch_claude_args` | `MIKE_LAUNCH_CLAUDE_ARGS` | empty | Operator flags prepended to every launch. Whitespace-separated in the env form. |
| `launch_worktree_config` | `MIKE_LAUNCH_WORKTREE_CONFIG` | unset | Colon-separated list of files carried into a worktree launch. |
| `launch_brief_inline_max` | `MIKE_LAUNCH_BRIEF_INLINE_MAX` | `800` | Prompts longer than this are staged to a private brief file the session is told to read. |
| `launch_channel_check_enabled` | `MIKE_LAUNCH_CHANNEL_CHECK_ENABLED` | `true` | Wait for the session's Remote Control channel after a launch. |
| `launch_channel_check_timeout_secs` | `MIKE_LAUNCH_CHANNEL_CHECK_TIMEOUT_SECS` | `90` | The channel check's wait budget. |
| `launch_channel_check_interval_secs` | `MIKE_LAUNCH_CHANNEL_CHECK_INTERVAL_SECS` | `3` | The channel check's poll interval. |
| `launch_warn_surfaces` | `MIKE_LAUNCH_WARN_SURFACES` | `8` | Warn when the backend already hosts this many surfaces; `0` disables. |
| `launch_max_surfaces` | `MIKE_LAUNCH_MAX_SURFACES` | `16` | Refuse a launch at this many surfaces; `0` disables. |
| `launch_auto_trust` | `MIKE_LAUNCH_AUTO_TRUST` | `false` | Record the folder as trusted in `~/.claude.json` before the session starts. Off unless you turn it on, so Claude Code asks about each new folder itself. |
| `launch_kill_wait_secs` | `MIKE_LAUNCH_KILL_WAIT_SECS` | `20` | How long `mike spawn kill` waits for the claude process to exit. |

### Feature configuration set at boot

These are boot defaults for features whose live values normally come from `/settings`. All of
them are off unless noted; the full feature inventory is in [`FEATURES.md`](FEATURES.md).

| Field | Env var | Default |
|---|---|---|
| `composition_enabled` | `MIKE_COMPOSITION_ENABLED` | `false` — also `--composition-enabled` |
| `weasel_intercept.enabled` | `MIKE_WEASEL_INTERCEPT__ENABLED` | `false` |
| `weasel_intercept.forbidden_words` | `MIKE_WEASEL_INTERCEPT__FORBIDDEN_WORDS` | empty |
| `reminders.enabled` / `.inject_lessons` / `.max_lessons_to_inject` | `MIKE_REMINDERS__*` | `false` / `false` / `10` |
| `recall_injector.enabled` / `.max_lessons_per_topic` | `MIKE_RECALL_INJECTOR__*` | `false` / `5` |
| `compress.enabled` / `.min_block_bytes` | `MIKE_COMPRESS__*` | `false` / `256` |
| `steerer.enabled` | `MIKE_STEERER__ENABLED` | `false` |
| `steerer.model` | `MIKE_STEERER__MODEL` | `openai/gpt-5.6-luna` |
| `steerer.openrouter_base_url` | `MIKE_STEERER__OPENROUTER_BASE_URL` | `https://openrouter.ai/api/v1` |
| `steerer.refocus.enabled` | `MIKE_STEERER__REFOCUS__ENABLED`, etc. | `false` |
| `steerer.correction.*` | `MIKE_STEERER__CORRECTION__*` | ignored: the LLM correction classifier is retired; a set `enabled` logs a startup warning |
| `steerer.judge.enabled` | `MIKE_STEERER__JUDGE__ENABLED` | `false` — no longer gates the turn judge; the runtime `judge_worker` feature toggle is the single gate |
| `session_judge.enabled` | `MIKE_SESSION_JUDGE__ENABLED` | `false` |
| `lesson_relations.enabled` | `MIKE_LESSON_RELATIONS__ENABLED` | `false` |
| `embedding.enabled` | **ignored** — use `--embedding` or `/settings` | `false` |
| `analysis.enabled` | `MIKE_ANALYSIS__ENABLED` | `true` — local diagnostics only, nothing leaves the machine |
| `lesson_worker.enabled` | `MIKE_LESSON_WORKER__ENABLED` | `true` — local |
| `harvester.enabled` | `MIKE_HARVESTER__ENABLED` | `true` — local |
| `retention_sweep.interval_secs` / `.orphan_batch_size` | `MIKE_RETENTION_SWEEP__*` | `3600` / `2000` |
| `channel_link_sweep.interval_secs` / `.batch_size` | `MIKE_CHANNEL_LINK_SWEEP__*` | `60` / `200` |

### Verifier input budget (runtime setting)

`steerer.verifier.input_budget` is edited at `/settings` (Steerer group) or through `PATCH /api/settings/steerer.verifier.input_budget`, and applies at the next verifier evaluation with no restart. It has no `MIKE_*` variable.

| Value | Effect |
|---|---|
| `verifier-input-v1` (default) | Caps each part of the verifier's user message and lists older tool calls as counts |
| `verifier-input-v1-wide` | The same, with roughly doubled caps |
| `uncapped` | Sends the whole message, as before the budget existed; about fifteen times more per call |

The setting only narrows what the already-enabled verifier sends. It does not enable the verifier or any content egress, and the complete tool-call list stays available under the session's tool uses.

`mike verifier-replay` rebuilds past verifier inputs for one arm (`--arm uncapped|verifier-input-v1|verifier-input-v1-wide`, from `--set <items.jsonl>`) and is a dry run unless `--execute --out <file>` is given; `--dump <file>` writes the inputs, `--max-usd` (default 15) stops a run, `--model` and `--reasoning-effort` override the live steerer values, `--prompt-file <file>` judges with that prompt in place of the stored one (to try a wording against graded history before it goes live), and `--parity-recorded <file> --out <file>` compares the rebuilt uncapped message with recorded calls without any egress. The rebuilt message includes the session's dossier block (notes already sent, what opened the turn, recent reactions) as of the replayed moment, and each report carries the verdict's `need`, an outcome that includes `need_below_level`, and `limit_at_the_time`: whether the notes sent to the session by then would have stopped the judge from being called. An item whose ask the operator did not write (a compaction request, a task notification) is reported as `machine_ask` and not judged, as on the live path. Otherwise the replay always calls the judge, so every item gets a verdict. Per-call records (request body, tokens, cost) are written to `<out>.calls.jsonl` and the per-item reports to `--out`, never to `llm.calls`.

### Dossier builder (runtime settings)

The `dossier_builder` toggle (off by default; switched on after deploy; applies immediately, `/settings` or `PATCH /api/settings/features/dossier_builder`) builds `memory.turn_facts` for every closed operator turn from captured data. It is local: no LLM call, no HTTP request, nothing leaves the machine. Five settings tune it, all in the Dossier group at `/settings`, read at each backstop tick with no restart and no `MIKE_*` variable:

| Setting | Default | Range | What it does |
|---|---|---|---|
| `dossier.flush_interval_ms` | `2000` | 250-60000 | Wait after a turn closes before its session is built, so a burst of closes is one batch |
| `dossier.batch_sessions` | `25` | 1-500 | Most sessions built in one batch |
| `dossier.backstop_interval_secs` | `60` | 10-3600 | How often the backstop looks for closes that sent no event, and for pending, stale or older-builder facts |
| `dossier.backstop_lookback_hours` | `48` | 1-720 | How far back, by turn creation time, the backstop looks |
| `dossier.session_settle_minutes` | `30` | 5-1440 | Quiet time before a turn with no next genuine message is recorded as `none_session_settled` |
| `dossier.entries.enabled` | `true` | bool | Derive the operator's standing directives and the assistant's promises and waits into `memory.dossier_entries` from captured prompts and the work registry. Local only |

`mike dossier-backfill [--from 2026-07-27T00:00:00Z] [--until <ts>] [--stage classify|facts|all] [--batch-sessions 50] [--execute]` classifies historical prompts and builds turn facts through the same code the worker uses. It is a dry run unless `--execute` is given: the dry run connects read-only, reports what it would write, and writes nothing. `--execute` refuses to run while any request since 2026-09-17 still carries an agent id on the primary lane (run `mike origin-backfill --execute` first). It prints one JSON line per batch and a final summary, and a second `--execute` run writes zero rows.

`mike dossier-parity` is read-only (it connects through the read-only pool). `--against <turns.jsonl>` compares the Rust genuine-message resolver with the J2 dataset: `--from`, `--cutoff` bound the population, `--source j2-text|stored` picks the stream, `--as-of cutoff|close` resolves each turn at `--horizon` (default now) or at its own close, `--max-diff N` exits 2 when more differences are found, and `--out <dir>` receives `parity-summary.json` and `parity-diff.jsonl`. `--dispositions` replays the turn-lifecycle table over `[--from, --cutoff)` and writes `dispositions-summary.json`, `dispositions-diff.jsonl` and `dispositions-sample-candidates.jsonl`.

`mike origin-backfill` re-lanes primary-lane requests that carry `x-claude-code-agent-id` (`--since` default 2026-09-17, `--until`, `--batch` default 500) to `workflow_subagent` or `subagent` with author `agent`, marks the operator turns those requests opened `opened_by = 'agent'`, and lists the operator turns an agent request closed in `closed-by-agent.jsonl`. It is a dry run unless `--execute` is given (which connects read-write); both modes write `summary.json` and `closed-by-agent.jsonl` to `--out`, and `--execute` also writes `relaned.jsonl` and `agent-opened.jsonl`, the old and new values needed to revert. A second `--execute` run writes nothing.

### Scheduled jobs (runtime settings)

| Setting | Default | Range | What it does |
|---|---|---|---|
| `learning_promote.enabled` | `true` | bool | Once an hour at half past, from 20:30 to 03:30 UTC, the proxy runs `scripts/learning-promote` from its working directory, which writes project memories and skills into the project checkouts on this machine. Default on because the systemd timer it replaces was on. Read every minute; a run already under way finishes. No LLM call, nothing leaves the machine |
| `learning_audit.enabled` | `true` | bool | Once on Monday, between 01:00 and 06:59 UTC, the proxy runs `scripts/learning-audit --weekly --sync` from its working directory. Read-only queries, a report under `artifacts/learning-audit`, published through Mike's own knowledge ingestion. One machine per database (job lease). Default on because the systemd timer it replaces was on. No LLM call |
| `compressor_loop.enabled` | `true` | bool | Once a day, between 04:30 and 06:59 UTC, the proxy runs `scripts/compressor-loop --days 7 --apply`. It can protect paths or stop forwarding; widening additionally requires current task/recovery evidence: `--evidence FILE`, or without the flag `~/.local/state/mike/compressor-loop/qualification.json` if it exists (the scheduled run reads that file). Evidence expires after seven days and is tied to the running build and policy, so without a current file, or after a deploy, the scheduled run cannot widen. One machine per database (job lease). No LLM call |
| `cargo_sweep.enabled` | `true` | bool | Every six hours (00:00, 06:00, 12:00, 18:00 UTC) the proxy runs `scripts/cargo-sweep-all`, which finds every cargo `target/` folder under the projects folder that holds this checkout (every project, its worktrees and nested targets such as `target/fleet`) and every cargo build-dir under `~/.cargo/build`, removes superseded builds to keep each under 40 GB (`CARGO_SWEEP_MAXSIZE_GB`), removes build output whose worktree branch is merged or whose checkout is gone, and fails when a swept volume has under 30 GB free (`CARGO_SWEEP_FLOOR_GB`). This machine only. Default on because the systemd timer it replaces was on |
| `lessons_commit.enabled` | `true` | bool | Once a day, between 01:30 and 03:59 UTC (03:30 in Johannesburg), the proxy runs `scripts/lessons-commit`, which commits the files the knowledge collector wrote under `.claude/lessons` in this checkout, only on `main` and only while no other repository operation is in progress. It never fetches or pushes. This machine only. Default on because the systemd timer it replaces was on. No LLM call |
| `usage_audit.enabled` | `true` | bool | Once on Sunday, between 06:00 and 10:59 UTC (08:00 in Johannesburg), the proxy runs `scripts/usage-audit.sh`, which reads the capture database and writes the weekly usage report under `~/.local/state/mike-audit`. The script exits 1 on purpose when an alarm trips and still writes its report; the proxy treats that exit as a finished run and shows the alarm lines as the job's detail on `GET /api/scheduled-jobs`. One machine per database (job lease). Default on because the systemd timer it replaces was on. No LLM call |

There is no `MIKE_REPO_ROOT`: the proxy uses the directory it was started in (the stack unit's `WorkingDirectory`, or the directory `deploy-proxy.sh` changes into). `GET /api/scheduled-jobs` lists every job with its last run, next run and any failure, and the `scheduled_jobs` row of `/api/capabilities` flags a failing or overdue job. A job that needs one machine per database takes a lease row named after the job in `llm.worker_leases`; with no machine identity file or no database it runs unleased. On a machine that does not hold the lease the job reports `state: "ok"` with `owner_machine_uuid` set to the machine that does; `off` always means the job's `<job>.enabled` setting is false.

### Semantic context (runtime settings)

Toggles `change_record` and `change_record_tail` (global, default **off**, applies immediately, `/settings` or `PATCH /api/settings/features/<name>`) switch on the change record. `change_record` starts a file watcher for each checkout a session is working in and lets the note, mail, check and brief handlers write an event; it reads and writes only local files and the database. `change_record_tail` adds the `since_you_last_looked` lines to the answers of `fleet_commit`, `fleet_check`, `write_session_note` and `send_session_message`, and a reminder with the same lines to the first request of a turn. Neither has a setting beyond the toggle. See [`CHANGE_RECORD.md`](CHANGE_RECORD.md).

Toggle `semantic_context` (global, applies immediately, default on) runs the semantic worker: embedder checks, items, floors, the pattern tagger, candidate grouping and drift. It changes no Claude traffic. Every embed call checks that the embedding URL of that call is a loopback host (`127.0.0.0/8`, `::1`, `localhost`); a remote URL is refused unless `semantic.allow_remote_embedding` is on, so no text leaves the machine by default. The "Semantic context" group on `/settings` holds `semantic.allow_remote_embedding` (false), `semantic.sweep_interval_secs` (300), `semantic.batch_turns` (64), `semantic.ending_chars` (1024), `semantic.inline_timeout_ms` (5000), `semantic.floor_quantile` (0.999), `semantic.floor_sample_turns` (300), `semantic.retrieval_scope` (`operator`), `semantic.retrieval_k` (3), `semantic.relapse_evidence` (`record`), `semantic.tagger_min_score` (0.05), `semantic.cluster_floor_quantile` (0.99), `semantic.cluster_min_members` (4), `semantic.cluster_min_sessions` (3), `semantic.drift_z` (3.0) and `semantic.drift_min_current` (30), all read each tick.

`mike semantic backfill --from <date> [--to <date>] [--execute]`, `embedders verify --since <time> [--execute]`, `anchors import --dir <fleet-mirror dir> [--execute]`, `floors recompute [--execute]`, `floors promote --pair <p> --from-id <id> [--rebundle] [--execute]`, `prune --embedder-key <key> [--execute]` and `group [--execute]` are dry runs unless `--execute` is given; `backtest retrieval|tagger|drift` are read-only.

### Turn judge promotion (runtime settings)

Five settings in the "Turn judge" group on `/settings/workers`, all read on the next cycle with no restart:

| Setting | Default | Range | What it does |
|---|---|---|---|
| `turn_judge.live_bundle_id` | `0` | 0- | The gated `eval.bundles` id whose configuration writes every turn's label. `0` keeps the r3 judge. Setting it is the promotion; `config.change_log` records it |
| `turn_judge.immediate` | `false` | bool | Judge a turn a few seconds after the operator's next genuine message. Shares eligibility and database ownership with the sweep and history worker; changes when turn text leaves the machine |
| `turn_judge.immediate_delay_ms` | `1500` | 0-30000 | Wait after a close before building the turn's facts and judging it |
| `turn_judge.live_window_days` | `7` | 1-60 | The sweep judges turns closed in this window; older turns belong to the history re-judge |
| `turn_judge.rejudge_daily_call_cap` | `300` | 1-5000 | Daily turn-history admission limit; `mike eval rejudge --daily-cap` overrides it for a run. Accounting is based on persisted verdict rows, as described below |

For a turn-history run, `calls_today` counts matching live verdict rows for its bundle and turn window written since both UTC midnight and the run's window end. Ordinary live judging and repeated verdict rows can consume this allowance. Provider failures can consume spend without adding a verdict row. An exhausted run remains `running` and waits for the UTC day to roll over. The shared LLM spend ceiling applies independently.

The session-end judge has its own group ("Session judge"), read on the next cycle:

| Setting | Default | Range | What it does |
|---|---|---|---|
| `session_judge.live_bundle_id` | `0` | 0- | The gated `session_judge` bundle whose configuration writes every session-end label. `0` keeps the legacy path |
| `session_judge.max_attempts_per_day` | `3` | 1-50 | Most session-judge calls one session can cost in a rolling 24 hours, per model and rubric; a session at the cap is skipped until its window rolls over |
| `session_judge.v3_lookback_hours` | `72` | 1- | The v3 judge picks up session endings that settled within this window, so promotion never re-judges history by itself |
| `session_judge.rejudge_daily_call_cap` | `200` | 1-2000 | Most session-end history re-judge calls per UTC day |

`mike eval rejudge --since <ts> [--until <ts>] [--daily-cap N] [--execute] [--status] [--cancel]` is a dry run unless `--execute` is given. The dry run prints eligible and ineligible counts, estimated calls and USD, days at the cap, the judge's slice and shared pool at the current ceiling, and the bundle it would use. It refuses when `turn_judge.live_bundle_id` is 0 or the bundle has no passing holdout gate. `--execute` inserts the `eval.rejudge_runs` row; the server's judge worker drains it, never the CLI.

### Limits on steering notes (runtime settings)

How many steering notes one session receives, set on `/settings` in the Steerer group. They apply to every steerer and to the turn guard.

| Setting | Default | Range | What it does |
|---|---|---|---|
| `steerer.session_note_limit` | `3` | 1-20 | Most notes one session receives inside the window. The first needs a need of medium or high, every later one needs high. At the limit no judge is called for that session |
| `steerer.session_note_window_minutes` | `60` | 5-1440 | How far back the limit looks |
| `steerer.wrong_note_mute_hours` | `24` | 0-168 | After a steerer's note to a session is labelled wrong, that steerer sends that session nothing for this long. 0 turns it off |

### Turn guard (runtime settings)

Toggle `turn_guard` (global, `LiveDisable`, default **off**) runs the turn-end guard: at each turn end it asks whether the turn relapsed after a correction, ended on a wait nothing backs, handed back an authorised step, or claimed done without observing the result. It sends captured prompt and response text to OpenRouter (its calls and the labeller's), so it ships off. In this release every answer is recorded in `steering.evaluations` as a shadow row and nothing is delivered. It also needs the seeded `turn-guard` steerer row enabled (`PATCH /api/steerers/{id}` with `{"enabled": true}`), which new sessions then auto-enrol in.

| Setting | Default | Range | What it does |
|---|---|---|---|
| `steerer.turn_guard.max_calls_per_day` | `150` | 0-1000 | Guard calls per UTC day, quorum calls included; at the cap the open questions are recorded `daily_cap` |
| `steerer.turn_guard.max_label_calls_per_day` | `80` | 0-1000 | Labelling calls per UTC day (billed to the eval role) |
| `steerer.turn_guard.recall_sample_pct` | `10` | 0-100 | Share of gate-closed turn ends the labeller also labels, so recall has a denominator |

Each question has a mode in `steering.question_policies` (`off` or `shadow`; `append` and `wake` belong to JC17), set on `/settings/steering`, with `PATCH /api/steering/guard/questions/{question}`, or with `mike guard mode <question> off|shadow`. Per-question confidence thresholds and floors live in the steerer row's `config`; the promotion bars are code constants. `mike guard report [--since] [--until] [--json]` prints the funnel, precision, recall and the exemption audit and exits 1 when a would-fire lands on a seat or template session; `mike guard replay-gates --since <ts>` recomputes the exemptions and gates over closed turns with no model call; `mike guard replay --since <ts> [--turns <ids.json>] [--labels <truth.json>]` replays the whole guard and reports per-question precision with Wilson intervals.

### Guard question promotion (runtime settings)

A guard question changes traffic only after an operator promotes it (`POST /api/steering/questions/{steerer_id}/{question}/promote`, the panel on `/settings/steering`, or `mike steer-questions promote --question Q --to append|wake [--override-reason TEXT] [--replaces-steerer N] --execute`), which re-runs the precision gate and refuses a failing gate (409, with the gate evidence) unless an override reason is given. Every change to or from `append` or `wake` writes a `steering.question_promotions` row and a `config.change_log` promotion row. Six settings in the "Steering: turn guard" group, read within a minute:

| Setting | Default | Range | What it does |
|---|---|---|---|
| `steerer.turn_guard.holdback_pct` | `10` | 0-50 | Share of a live question's would-fires withheld at random as the control arm (a per-question override lives in `steering.question_policies.holdback_pct`) |
| `steerer.turn_guard.wake_grace_relapse_secs` | `30` | 0-600 | Wait after the turn ends before a relapse wake is pushed |
| `steerer.turn_guard.wake_grace_wait_secs` | `90` | 0-900 | Wait before an unbacked-wait wake is pushed |
| `steerer.turn_guard.wake_cap_per_session_hour` | `3` | 0-20 | Guard wakes per session per rolling hour |
| `steerer.turn_guard.auto_demote` | `true` | bool | Hourly check that demotes a live question on lost precision, measured harm, or two weeks over the wrong-wake ceiling; demotion only reduces traffic changes |
| `steerer.turn_guard.wrong_wake_ceiling_per_week` | `2` | 0-50 | Wrong wakes a week above which a wake question drops to append |

Every live fire is appended to the session's next request at once; a wake also gets a due time, and the quiescence worker's 60 s tick pushes it only if it is still undelivered, the session is idle with an open Remote Control stream, nothing now backs the wait, and the hourly cap allows it, recording why otherwise. `mike steer-questions status|effect|verify-arms|demote` read and act on the same state.

### Cross-session calibration and directive proposals (runtime settings)

The cross-session worker re-reads its settings every 60 s and, on each new `eval.rollups` generation, runs the threshold pass and (with the `directive_proposer` toggle on; global, default **off**) the proposer. Group "Steering: cross-session calibration":

| Setting | Default | What it does |
|---|---|---|
| `guard.calibration.input_mode` | `off` | `live` puts a `cal-v1` calibration block (per open question: labelled candidate precision, the corrected share, a fact-conditioned rate, the frustration-close share) and a fixed paragraph in the turn guard's call. Switch on only after `mike guard replay --calibration` shows precision with the block at or above precision without it |
| `guard.calibration.min_labelled` | `20` | Labelled candidates a project needs for its own line; below it the all-projects line is used |
| `guard.calibration.window_days` | `30` | Stated in the block's header |
| `guard.threshold.mode` | `off` | `apply` sets each question's threshold to the lowest confidence whose Wilson lower bound clears the question's bar, within its floor and the ceiling; each applied change writes `steering.guard_thresholds` and a `config.change_log` row with via `calibration` |
| `guard.threshold.min_labelled` | `30` | Labelled candidates needed before a threshold is calibrated |
| `guard.threshold.hysteresis` | `0.05` | A candidate closer than this to the threshold in force is held |
| `guard.threshold.cooldown_days` | `7` | A threshold changed more recently is held |
| `guard.threshold.ceiling` | `0.95` | Upper bound on any calibrated threshold |
| `guard.threshold.allow_lowering_for_wake` | `false` | Off keeps a wake question's threshold at or above its static value |
| `guard.threshold.weight.operator` | `1.0` | How much one operator label counts toward a threshold's labelled total and precision (0 to 1; 0 ignores the source) |
| `guard.threshold.weight.session_reply` | `1.0` | The same for a label taken from the session's own reply to a delivered note |
| `guard.threshold.weight.session_end` | `0.8` | The same for a label inferred when the session ended |
| `guard.threshold.weight.next_message` | `0.5` | The same for a label inferred from the operator's next message, the weakest source |
| `guard.threshold.steerer_bar` | `0.75` | Wilson lower-bound precision the verifier and monitor-over-sleep steerers' fires must reach before their confidence threshold is raised; with no confidence level clearing it the pass records `below_bar` and keeps the threshold in force |
| `directives.proposer.sources` | `["repeated_directive", "confirmed_handback"]` | Proposer sources; the tag and pattern sources are reported as unavailable |
| `directives.proposer.window_days` | `30` | Evidence window |
| `directives.proposer.min_sessions` | `2` | Sessions a directive must be restated in, one of them in a correcting message |
| `directives.proposer.min_confirmed` | `3` | Confirmed authorised hand-backs needed |

Proposals are listed and rejected on `/memory`; nothing is injected. `mike guard calibration [--as-of TS]` prints the snapshot; `mike directives backtest --corrections <jsonl> --turns <tsv>` reproduces `p8/directive_backtest.py`; `mike directives propose [--execute]` runs one proposer pass.

### Evaluation and dossier commands

Every command below reads the database named by the boot config. Commands that list `--execute` default to a dry run. `mike eval run` instead executes and records model calls unless `--dry-run` is given; use `--max-usd` to cap a run's spending. `mike eval gate` records its decision when applied to an evaluable run.

| Command | What it does |
|---|---|
| `mike eval import fleet-mirror --dir <dir> [--control-turn N ...] [--execute]` | Imports the fleet-mirror seed labels into `eval.labels` with an `eval.import_batches` row; `--control-turn` (default 16368) names turns whose labels must match the seed |
| `mike eval import holdout --dir <run dir> --batch <name> [--drop-split-conflicts] [--execute]` | Imports a holdout labelling run (`manifest.json`, `sample-blind.json`, `import.jsonl`) as one batch; refuses a rubric mismatch |
| `mike eval withdraw-batch <batch> --reason <text> [--execute]` | Withdraws every label of a batch, keeping the rows for the audit |
| `mike eval rollup [--day D \| --from D --to D] [--execute] [--verbose]` | Runs the daily rate rollup (all families) and the alarm evaluation for a day or a range, as the worker does |
| `mike eval rates --family accuracy\|label_mix\|base_rate\|steer_calibration [--role R] [--label-source S] [--class C] [--from T --to T] [--json]` | Prints stored rates; with `--from/--to` it computes the family ad hoc in a rolled-back transaction |
| `mike eval alarms list [--state open\|cleared\|all]` | Lists alarms |
| `mike eval alarms backtest --kind <kind> --from <ts> --to <ts> [--step 1d\|1h]` | Replays one alarm's predicate over a past window, read-only |
| `mike eval run [--role turn_judge\|session_judge] [--bundle N] [--rubric R] [--builder B] [--slices a,b] [--prompt-file F] [--model M] [--split dev\|holdout] [--tiers seed,gold,relayed] [--methods ...] [--holdout-batch B] [--labels-as-of T \| --same-labels-as RUN] [--limit N] [--resume RUN] [--dry-run]` | Runs a judge configuration over labelled items and scores it; the run and its items land in `eval.runs` and `eval.run_items`, billed to the eval budget role |
| `mike eval gate --candidate RUN [--baseline RUN] [--noise RUN] [--role turn_judge\|session_judge] [--rule promotion\|e-slice] [--override-reason TEXT] [--json]` | Applies the promotion floors to a run and records an `eval.gates` row; exit 0 pass, 1 fail, 2 incomplete |
| `mike eval show <id> [--gate] [--items] [--json]` | Prints a run (or, with `--gate`, a gate) whole, optionally with every item |
| `mike eval rejudge ...` | See the turn judge promotion section above |
| `mike eval session-end-defaults [--source fleet_mirror/session_ends] [--tiers seed,gold] [--json]` | Scores the rule-based session-end label against the seed and gold labels, with Wilson intervals |
| `mike dossier-render --session <uuid> [--as-of T] [--mode knowledge] [--audience none\|turn_judge\|session_judge\|steerer] [--turn N] [--slices ...] [--sections ...] [--budget-chars N] [--full-view] [--out F]` | Renders the dossier view a model would read for a session, read-only; `--e-parity <file>` instead checks the renderer against a recorded judge input |

`mike eval run --bundle N` evaluates the selected bundle's stored configuration and records that exact bundle ID on the run. Do not combine it with `--rubric`, `--builder`, `--slices`, `--prompt-file`, or `--model`; those options construct a configuration when no bundle is selected. `--resume RUN` uses the original run's frozen bundle and cannot be combined with `--bundle`. Choose the matching `--role` for the bundle.

Historical rejudging requires a passing holdout gate for the selected live bundle. Development runs and a `live_ungated` promotion do not satisfy that requirement. Import an independently labelled holdout package before evaluating and gating the live bundle; changing existing development labels to the holdout split does not provide independent evaluation evidence.

### MCP bridge

| Variable | Default | What it does |
|---|---|---|
| `MIKE_INSPECT_URL` | `http://127.0.0.1:9989` | Where `mike-mcp` reaches the inspect bind |
| `MIKE_MCP_PROFILE` | `core` | `full` ships the whole tool catalogue; see [`MCP_BRIDGE.md`](MCP_BRIDGE.md) |

## Persistent storage (Postgres)

Persistence defaults on. You have two options.

### Option A — bring your own Postgres

Point mike at any reachable Postgres 19 with `pgvector` — local, LAN, or managed.

```bash
createuser mike --pwprompt
createdb mike --owner=mike
```

Then export the target explicitly for commands started from this shell, or save it through
`mike setup database`. For a checkout `.env`, import it with `mike import-dotenv .env`:

```bash
export MIKE_DATABASE_URL='postgres://mike:your_password@your-host:5432/mike'
```

Any connection string the `sqlx` Postgres driver accepts works, including `sslmode=require` and
managed providers (RDS, Supabase, Neon).

**What it creates.** The migrations create about two dozen schemas: `agents`, `config`,
`launch`, `llm`, `mailbox`, `memory`, `messages`, `overrides`, `projects`, `prompt_kinds`,
`remote_control`, `requests`, `responses`, `rules`, `session_judges`, `sessions`, `statements`,
`steering`, `sync`, `system_prompts`, `tools`, `work`, `workspace`, plus the `mike` views schema
(renamed from `claude_proxy` by migration `20260718000000`). Several of these names are generic
enough to collide with another application's own schemas. Give mike a dedicated database rather
than relying on schema-name isolation. Full detail: [`SCHEMA.md`](SCHEMA.md).

**Privileges exceed plain `CREATE SCHEMA`.** The baseline migration runs `CREATE EXTENSION IF
NOT EXISTS` for `vector`, `pgcrypto` and `pg_trgm`, which typically needs superuser or a
managed-cloud equivalent such as `rds_superuser`. Grant that up front, or pre-create the three
extensions yourself.

**What happens on startup.** mike connects, runs the `sqlx` migrations (idempotent; `sqlx`
tracks the applied set in `_sqlx_migrations`), and starts capturing. Postgres is the source of
truth for completed traffic: every inspect-API read for a terminal request queries it directly.
Request and response bodies are stored in Postgres, not in a separate blob store. The bounded
in-memory store is a live-capture staging pool for in-flight and streaming state only; entries
leave it once they have been snapshotted. Nothing is rehydrated into memory at boot — historical
reads go straight to Postgres.

If the primary database is unreachable, mike falls back to `local_database_url` when one is
configured and `db_fallback_enabled` is on. With no fallback, boot fails with the redacted URL
in the error.

### Option B — skip persistence

Set `MIKE_PERSIST_ENABLED=0` and run the binary directly:

```bash
MIKE_PERSIST_ENABLED=0 mike serve
```

Use `mike serve` rather than a bare `mike`: with a TTY on stdout a bare `mike` opens the
control-centre TUI instead of serving. From a source tree, `scripts/cx run -p mike -- serve`
preloads the environment raw `cargo` needs.

`scripts/start-proxy.sh` probes for a database and is not built for this mode. Captured traffic
lives only in the in-memory staging pool and does not survive a restart.

### Running DB-backed integration tests

Tests that touch the persistence layer use `TestDb::acquire` / `acquire_isolated` /
`acquire_shared` from `crates/mike/src/store/persistence/test_support/`. Point
`MIKE_TEST_ADMIN_URL` at an **admin** database:

```bash
MIKE_TEST_ADMIN_URL=postgres://mike:your_password@your-host:5432/postgres make test-db
```

`TestDb::bootstrap()` reads that variable and skips early when it is unset, so the fast test
tier stays green on a Postgres-less workstation. It refuses outright if the admin URL names the
development database itself, so the suite cannot be pointed at real data.

`acquire_isolated` provisions its own `..._test_<uuid>` database from a migrated template and
drops it on cleanup. `acquire_shared` reuses one `..._test_run_<run-id>` database across a whole
run — cheaper, but those tests share state within the run.

## Manual launch (no script)

To run the pieces in separate terminals instead of `scripts/start-proxy.sh`:

```bash
# Terminal 1 — mike, with its own SPA bind turned off so vite can own :9990
MIKE_SPA_ENABLED=0 mike serve

# Terminal 2 — the vite dev SPA host
INSPECT_BACKEND=http://127.0.0.1:9989 MIKE_FRONTEND_PORT=9990 \
    bash -c 'cd frontend && pnpm run start'
```

`MIKE_SPA_ENABLED=0` matters: without it mike binds `:9990` itself and vite cannot start there.

`frontend/vite.config.ts` reads `INSPECT_BACKEND` (default `http://127.0.0.1:9989`) and
`MIKE_FRONTEND_PORT` (default `9990`) from `process.env`. `pnpm run start` maps to `vite dev`.

If you do not need hot reload, skip the second terminal entirely: `mike serve` alone serves the
SPA it was compiled with on `:9990`, provided the binary was built with a `frontend/build`
present.

From a source tree without an installed binary, replace `mike serve` above with
`scripts/cx run -p mike -- serve`.

## See also

- [`GETTING_STARTED.md`](GETTING_STARTED.md) — the first-run path this doc assumes
- [`FEATURES.md`](FEATURES.md) — the runtime feature toggles and their defaults
- [`SECURITY.md`](SECURITY.md) — `inspect_allowed_hosts`, `spa_origins` and the trust boundary
- [`LOCAL_DB_FALLBACK.md`](LOCAL_DB_FALLBACK.md) — the fallback database and its resync
- [`TROUBLESHOOTING.md`](TROUBLESHOOTING.md) — common failure modes and fixes

## Evaluation metrics settings (group "Evaluation")

All are live-read by the eval metrics worker on its next 15-minute tick; none mutates traffic or sends data anywhere.

| Key | Default | Meaning |
|---|---|---|
| `eval.rollup.enabled` | `true` | Off stops the rollup and every alarm at the next tick |
| `eval.rollup.hour_utc` | `3` | The rollup runs on the first tick at or after this UTC hour |
| `eval.alarm.label_mix.min_turns` | `50` | Minimum judged real turns in the 7-day window (baseline 3x) |
| `eval.alarm.label_mix.min_tvd` | `0.20` | Total variation distance floor |
| `eval.alarm.label_mix.min_chi2` | `40.0` | Chi-squared floor, df 3 |
| `eval.alarm.unjudged_backlog.min_turns` | `10` | Unjudged turns closed over 6 h ago |
| `eval.alarm.judge_stalled.min_waiting` | `5` | Turns waiting over 30 minutes |
| `eval.alarm.judge_stalled.hours` | `6` | Hours with no judge verdict |
| `eval.alarm.budget_refusals.min_share` | `0.5` | Share of a role's calls refused over 24 h |
| `eval.alarm.budget_refusals.min_refused` | `50` | Minimum refused calls |
| `eval.alarm.change_without_gate.grace_hours` | `24` | Time allowed to gate a bundle change |
| `eval.alarm.judge_call_loop.min_calls` | `50` | Judge calls in the window before the call-loop alarm is considered |
| `eval.alarm.judge_call_loop.max_calls_per_row` | `3.0` | Calls per verdict row above which the call-loop alarm fires |
| `eval.labels.holdout_percent` | `30` | Share of turns, labelled for the first time by a gold or relayed label, that fall in the holdout split |
| `eval.relay_labels_enabled` | `true` | Lets a session record the operator's quoted words about a turn or session as a relayed label (`relay_label`) |

`steerer.quiescence_minutes` selects nothing on its own any more: the idle sweep evaluates only steerers whose config sets `quiescence_after_secs`, and a value of 0 there copies this default.

`llm.budget.eval.share_pct` (default 0) is the evaluation role's guaranteed share of the LLM limits. At 0 the offline `mike eval` runs draw only on the unguaranteed pool, so they can never take a production worker's share.
