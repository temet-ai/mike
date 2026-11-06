---
title: Optional dependencies and launchers
status: active
created: 2026-07-20
updated: 2026-11-06
summary: What each optional external service buys you, what mike does without it, what it is commonly but wrongly assumed to need, and the cc shell launcher for captured restartable sessions.
supersedes: the first-run and feature-toggle material this doc used to carry
---

# Optional dependencies and launchers

`mike` needs **one** external service — PostgreSQL — and that gives you the whole capture and
inspect product. Everything else on this page is optional, and none of it is needed for a
working install.

This page is the honest inventory of the optional pieces. For the first run see
[`GETTING_STARTED.md`](GETTING_STARTED.md); for every optional *feature* and its default see
[`FEATURES.md`](FEATURES.md); for every config key see [`CONFIGURATION.md`](CONFIGURATION.md).

## Required: exactly one thing

**PostgreSQL 19 with the `vector` (pgvector), `pgcrypto`, and `pg_trgm` server packages for the installed provider.**

Mike requires PostgreSQL major version 19. Capture runs 19rc1 with pgvector 0.8.7.
Older and future majors are rejected before migrations. Fresh managed installs,
CI, and disposable tests build the same image using
`scripts/capture-db/Dockerfile.pg19`; the installed binary embeds that recipe.
For existing older clusters, perform a separate PostgreSQL upgrade first (the
capture cluster used `scripts/capture-db/upgrade-to-19.sh`).


- Persistence defaults on. If the primary database is unreachable at boot, mike falls back to
  `local_database_url` when one is configured and `db_fallback_enabled` is on (the default) —
  see [`LOCAL_DB_FALLBACK.md`](LOCAL_DB_FALLBACK.md). With no fallback configured, boot fails
  with the redacted URL in the error.
- The migration run needs privileges to `CREATE EXTENSION` (superuser or a managed-cloud
  equivalent such as `rds_superuser`), or pre-create the three extensions yourself.
- Managed path: `mike database provision` uses the embedded PostgreSQL 19 image recipe and
  Compose resource and saves a private credential. It needs Docker plus Compose. Run
  `mike serve` afterward to apply embedded migrations.
- Existing PostgreSQL: supply a private URL and run `mike database check`. Provider-specific
  extension and role restrictions must pass the checks; compatibility is not assumed from
  a hosting brand. See [`INSTALLED_DATABASE.md`](INSTALLED_DATABASE.md).
- A no-database mode exists (`MIKE_PERSIST_ENABLED=0`, run `mike serve` directly) but is an
  ephemeral opt-out: captured traffic dies with the process.

## Optional dependency matrix

| Dependency | What it powers | Without it | How to be sure it is off |
|---|---|---|---|
| **OpenRouter API key** (`MIKE_STEERER_OPENROUTER_API_KEY`, or `OPENROUTER_API_KEY`) | The LLM features: steerer fleet (including the turn guard), session judge, judge worker, the offline `mike eval run` judge runs, lesson-relation detection. **Enabling any of these sends captured conversation text to `openrouter.ai`.** | Those workers log a warning at boot and never spawn. The forward path is unaffected. | Leave the key unset. All of the features are also off by default in `/settings`. |
| **Embedding server** (OpenAI-shaped `/embeddings`; default `http://127.0.0.1:7997`, model `nomic-embed-text-v1.5`; with Ollama, `http://127.0.0.1:11434/v1` and `nomic-embed-text`, which `mike setup embeddings` sets up) | Semantic recall ranking (`recall_similar_turns`, `recall_project_context`) and lesson dedup/promotion. | Recall falls back to Postgres `pg_trgm` text search — still works, lower-quality ranking. | `embedding.enabled` defaults off. If you turn it on, a **remote or hosted** `embedding.url` means the text being embedded leaves your machine; a local server does not. `MIKE_EMBEDDING__*` environment variables are **ignored** — use `/settings` or the `--embedding` CLI flag. |
| **Docker** | The convenience local Postgres only. | Bring your own Postgres, or run `mike db native` for the commands that set one up without Docker. | n/a — never required by the proxy itself |
| **`proxy-forward` sidecar** (`:9987`) | Capturing Claude Code's Remote Control WebSocket channel, and failover to Anthropic while the proxy restarts. This is an HTTPS MITM with its own generated CA. | Normal `/v1/*` capture through `:9988` is complete; only the Remote Control channel goes unobserved. | Don't start it. All `MIKE_FORWARD_*` config is sidecar-local. |
| **Node ≥ 25 + pnpm** | Building the browser UI. `mike` serves the SPA it was compiled with; `vite dev` is the hot-reload development path. | The inspect REST API (`:9989`) and every MCP tool still work. | Build mike without a `frontend/build` present, or set `MIKE_SPA_ENABLED=0`. |
| **`rg` (ripgrep) on PATH** | The on-demand subsystem-manifest resolver used by the reminders feature. | That one resolver returns a typed error; nothing else is affected. | none needed |
| **OTel collector** | Trace export. **Spans leave the machine when this is set.** | No traces exported. | Leave `MIKE_OTEL_ENDPOINT` unset (the default). |
| **A terminal backend** (herdr, tmux, or a Hyprland terminal) | `mike spawn`, which launches Claude Code sessions into a terminal surface. | Nothing else is affected; the launch verbs refuse until a backend is chosen. | `launch_backend` defaults to `none`. See [`LAUNCH_DOMAIN.md`](LAUNCH_DOMAIN.md). |

## Things you do NOT need

- **cmux, or any particular terminal multiplexer.** The fleet, mailbox, and workspace-roster
  features are entirely Postgres-backed and process-agnostic. Cross-session messaging works
  between any two Claude Code sessions pointed at the same proxy. A terminal backend is needed
  only for the `mike spawn` launch verbs.
- **Redis** — not a dependency anywhere in the codebase.
- **Ollama specifically** — any server speaking the OpenAI-shaped embeddings API works.
  Embeddings always run out-of-process.
- **An Anthropic API key for the proxy** — mike re-originates your Claude Code traffic with the
  credentials Claude Code already sends; it holds no key of its own.
- **`gh`, `jq`, Homebrew** — development-workflow tools for this repo, not runtime dependencies.

## Launching a captured session: `mike claude` and the shell function

`mike claude` starts Claude Code through mike. When `launch_use_sidecar` is on (the default is off;
the stored `claude_route` choice does not turn it on), the forwarding sidecar answers on `:9987` and
its certificate authority file exists, it sets `HTTPS_PROXY`, `NO_PROXY` and `NODE_EXTRA_CA_CERTS`
so that mike captures every request and the Remote Control channel, and it adds `--remote-control`
so that mike can restart the session later by pushing into that channel. Otherwise it sets
`ANTHROPIC_BASE_URL` to the proxy and says so. `--no-remote-control` opts out for one launch.

`mike claude-code shell` writes a short function (default name `cc`) into the startup files of
the shells installed on the machine, so that you type `cc` instead of `mike claude`; see
[`GETTING_STARTED.md`](GETTING_STARTED.md#connecting-claude-code). The setup wizard offers the
same function, and `mike uninstall` removes it.

The check runs once at launch: a session started while the sidecar was down stays uncaptured
for its whole life. Restart it once the sidecar is up.

### The developer's `cc` in `scripts/shell/cc.zsh`

The repository also has `scripts/shell/cc.zsh`, which defines `cc` and `ccdirect` for working in
a checkout. Source it from `~/.zshrc`, pointing at your own checkout:

```zsh
_temet_cc_lib="<repo>/scripts/shell/cc.zsh"
[ -r "$_temet_cc_lib" ] && source "$_temet_cc_lib"
```

It asks `mike forward ca-path` where the certificate lives and only falls back to the checkout's
`.deploy` folder (`TEMET_PROXY_REPO` overrides the checkout path) when no mike is installed. It
also adds `--dangerously-load-development-channels server:mike` and `--chrome`, and `ccdirect`
lets the Remote Control channel bypass the sidecar, which is the A/B test for a sidecar that
breaks replies. Do not use it together with a function written by `mike claude-code shell` under
the same name.

## See also

- [`GETTING_STARTED.md`](GETTING_STARTED.md) — first-run walkthrough
- [`FEATURES.md`](FEATURES.md) — every optional feature, its class, and its default
- [`SECURITY.md`](SECURITY.md) — what leaves the machine, and what the binds expose
- [`CONFIGURATION.md`](CONFIGURATION.md) — every config key and its precedence
- [`TROUBLESHOOTING.md`](TROUBLESHOOTING.md) — common failures

Database maintenance and PostgreSQL 19-specific SQL guidance: [PostgreSQL 19 operations](development/postgresql-19.md).
