---
title: Getting Started
status: active
created: 2026-07-07
updated: 2026-11-07
summary: One correct path from a fresh clone to a captured Claude Code request, with prerequisites, database options, security expectations, and what each process does once the stack is up.
---

# Getting Started

`mike` is a Rust reverse proxy that sits between Claude Code and `api.anthropic.com` and
captures every request and streaming response. This is the path from a fresh clone to your
first captured request. For the project overview see [`../README.md`](../README.md); for design
rationale see [`ARCHITECTURE.md`](ARCHITECTURE.md); for what's planned next see the
[Mike Roadmap](https://github.com/orgs/temet-ai/projects/1).

## Before you rely on it

Two things to know before you point real work at mike.

**mike ships as `0.1.0-alpha.N` archives** for Linux x86_64 and ARM64 (glibc 2.35 or newer)
and Apple Silicon macOS, through Homebrew (`brew install temet-ai/mike/mike temet-ai/mike/mike-mcp`) and
mise; [Install mike](user-guide/install.md) has the commands. There is no curl installer.
Everything below is a source checkout and the development stack. The release route is in
[`RELEASING.md`](RELEASING.md).

**The inspect API has no authentication.** Any process on the machine can read every captured
prompt and response, change any runtime setting including your OpenRouter API key, and start a
Claude Code session through `POST /api/launches`. Run mike on a machine you alone use, bound to
loopback. [`SECURITY.md`](SECURITY.md) is the full trust boundary.

**A fresh install only captures.** It forwards what Claude Code sent, returns what Anthropic
returned, and sends nothing to a third party. Every feature that rewrites traffic or calls out
to another service is off until you turn it on in `/settings`. See [`FEATURES.md`](FEATURES.md).
An install that already has settings stored keeps them — a default change never rewrites a
stored value.

## What you need

### Required

| Prerequisite | Why | Get it |
|---|---|---|
| **Rust toolchain** | Builds the `mike` / `mike-mcp` binaries. `rust-toolchain.toml` pins the channel (1.95.0) — no manual version pick needed. | [rustup.rs](https://rustup.rs) |
| **Node.js ≥ 25** | Builds the SvelteKit SPA (`frontend/package.json` `engines`). | [nodejs.org](https://nodejs.org) or a version manager: [nvm](https://github.com/nvm-sh/nvm) / [fnm](https://github.com/Schniz/fnm) |
| **pnpm** | Frontend package manager. The `packageManager` field in `frontend/package.json` pins the exact version — use that one. | `corepack enable` (bundled with Node), or [pnpm.io/installation](https://pnpm.io/installation) |
| **Claude Code CLI** | The client you point at the proxy. | [Claude Code docs](https://docs.anthropic.com/en/docs/claude-code) — `npm i -g @anthropic-ai/claude-code` |
| **PostgreSQL 19 with pgvector** | Persistence defaults on. The easiest path is `mike db up` below — no separate install needed if you have Docker. | [postgresql.org/download](https://www.postgresql.org/download/) if bringing your own, or `mike db native` |
| **Docker** (for `mike db up`) | Runs the Postgres container. Docker Desktop, OrbStack and Colima all work; `docker compose` or `docker-compose` is used. | [Get Docker](https://docs.docker.com/get-docker/) |

### Optional, and off by default

Neither of these does anything until you enable the feature that uses it. Both change what
leaves your machine.

| Prerequisite | What enabling it gets you | What it costs |
|---|---|---|
| **Embedding server** | Semantic ranking for recall and lesson dedup. Any server speaking the OpenAI-shaped `/embeddings` API works; the default URL is `http://127.0.0.1:7997` (Infinity) and Ollama's OpenAI-compatible endpoint also qualifies. Without it, recall falls back to Postgres text search. | If you point `embedding.url` at a remote or hosted endpoint, the text being embedded leaves your machine. A local server does not. |
| **OpenRouter API key** | The LLM features: steerer fleet (including the turn guard), session judge, per-turn judge worker, the offline `mike eval run` judge runs, lesson-relation detection. | These send captured conversation text to `openrouter.ai`. All of them are off by default, and all of them are inert without a key. |

Full table of every optional feature with its class and default: [`FEATURES.md`](FEATURES.md).

## Quick start

### 1. Get a database up

```bash
mike db up
```

This needs only Docker (Docker Desktop, OrbStack or Colima) and the `mike` binary, not the
source checkout and not a `psql` on the host. It writes a compose file embedded in the binary
to `~/.local/state/mike/db/docker-compose.yml`, builds and starts `mike/postgres-pgvector:19rc1-0.8.7` as the
container `mike-db` bound to `127.0.0.1:6666` only, keeps the data in the docker volume
`mike_pg_data`, waits until the database is healthy, and applies mike's own migrations. The
generated password is stored in the per-user config file `~/.config/mike/config.toml`
(`$XDG_CONFIG_HOME/mike/config.toml` when that is set) together with `database_url`, and is
never printed. Running it again changes nothing and keeps the data. `--port 6700` chooses
another port; a port that another program already holds is reported before anything starts.

| Command | What it does |
|---|---|
| `mike db status` | Shows the docker daemon, the container, the volume and the configured URL (password hidden). |
| `mike db check [url]` | Connects and checks PostgreSQL 19, the `vector`, `pgcrypto` and `pg_trgm` extensions (installed, or creatable by that user), and the migration state. Without a URL it checks the configured one. `--save` keeps a URL that passes. |
| `mike db down` | Stops and removes the container. The volume, and with it the data, stays; `mike db up` starts it again. |
| `mike db native` | Prints the exact commands for PostgreSQL without Docker on this machine. On macOS with Homebrew, `--yes` lets mike run them, since brew needs no root. On Linux the package, `initdb` and role commands need root, so mike prints them and never runs `sudo`. |

`mike db up`, `down` and `native --yes` run no command when `MIKE_SANDBOX=1` is set.

**Bring your own Postgres instead:** run `mike db check postgres://user:password@host:5432/mike --save`
against any reachable Postgres 19 with `pgvector`. mike creates its own schemas and runs
migrations on startup; see [`SCHEMA.md`](SCHEMA.md). The migration run needs privileges to
`CREATE EXTENSION` for `vector`, `pgcrypto` and `pg_trgm` (superuser, a trusted-extension
grant, or a managed-cloud equivalent such as `rds_superuser`), or you pre-create those three
extensions yourself; `mike db check` reports which case applies.

### 2. Dev stack only: put the URL in `.env`

`scripts/start-proxy.sh` reads `MIKE_DATABASE_URL` from `.env` in the repo root, not from the
per-user config file. In a checkout, either copy the URL into `.env` or use the fallback
database helper `scripts/local-db/up.sh --config PRIVATE_CONFIG` (see [`LOCAL_DB_FALLBACK.md`](LOCAL_DB_FALLBACK.md)
for its caveats):

```bash
MIKE_DATABASE_URL=postgres://postgres:postgres@127.0.0.1:6666/mike
```

The `mike` binary does not read a `.env` file itself; `mike config import-env [PATH]` (default
`.env`) copies one into the per-user file. See [`CONFIGURATION.md`](CONFIGURATION.md).

### 3. Start the stack

```bash
mike serve
```

This applies embedded migrations and runs the proxy, inspect API and bundled UI.

For the development stack in a checkout, `./scripts/start-proxy.sh` is a separate workflow
and does not use the installed configuration. It builds each Rust component, then cuts over
only what changed: it reinstalls `mike-mcp` into `~/.local/bin`, deploys `mike`, and starts the
`proxy-forward` sidecar and a `vite dev` SPA. Where systemd is available these run as user
units named `mike-stack-*` that outlive the script and restart on failure; without it
(`MIKE_STACK_SUPERVISOR=nohup`, or macOS) the SPA runs in the foreground of that terminal and
Ctrl-C stops only it. It does not start Storybook
(`systemctl --user start mike-stack-storybook`). Stop everything with
`./scripts/stop-proxy.sh`.

### 4. Point Claude Code at the proxy

In another terminal:

```bash
ANTHROPIC_BASE_URL=http://localhost:9988 claude
```

### 5. Open the inspection UI

<http://localhost:9990> — everything you type to Claude Code shows up live, grouped by session.

## The launcher and the database

`persist_enabled` defaults to `true`, and `scripts/start-proxy.sh` probes the database before it
starts anything. When the primary `MIKE_DATABASE_URL` host is unreachable it does **not**
exit immediately:

- If `MIKE_LOCAL_DATABASE_URL` is set, reachable, and `MIKE_DB_FALLBACK_ENABLED` is not `0`,
  the script proceeds and mike falls back to that database at boot.
- Otherwise, on a terminal, it offers **retry / start anyway / abort** (Enter aborts). "Start
  anyway" proceeds through the build and smoke test, which fail if the database has not come
  back by then.
- Without a controlling terminal — an agent, a background job, CI — it takes the abort path and
  exits 1.

## Running without a database

The `mike` binary supports `MIKE_PERSIST_ENABLED=0`, which keeps captured traffic only in the
bounded in-memory staging pool (`max_requests`, default 1000) and loses it on restart. Run it
directly rather than through the launcher script:

```bash
MIKE_PERSIST_ENABLED=0 mike serve
```

Use `mike serve`, not a bare `mike`: with a TTY on stdout a bare `mike` opens the control-centre
TUI instead of serving. From a source tree without an installed binary, use
`scripts/cx run -p mike -- serve`, which preloads the environment raw `cargo` needs.

Treat this as an ephemeral mode for quick disposable inspection, not the standard path.

## The setup wizard

Running `mike` with no subcommand on a terminal opens a control centre. It starts on the setup
wizard when persistence is on and no `database_url` is configured, and again at every start until
the wizard has been finished (the config file holds `setup = "unfinished"` until the last step is
reached, then `setup = "done"`). `mike setup` opens the wizard on a terminal at any time, and
`mike setup <step>` opens it at one step: `welcome`, `machine`, `database`, `service`, `claude`,
`features`, `models`, `embeddings`, `terminal` or `ready`. The key `f` leaves the wizard and keeps every
answer given so far, `Esc` goes back to the home screen and `q` quits.

Every step except the first, second and last does the same things in the same order: it shows what it found on this machine, offers
the choices that work here with one marked recommended, shows the exact command, runs it only when
you press Enter on the choice, and ends with a list of what it checked. A command that needs root
is printed for you to run, since mike never runs `sudo`. `r` checks the step again, `s` skips it
and the left arrow goes back. A field you type into is entered with `e` or by pressing Enter on a
choice that needs text, and while you type, no key acts as a shortcut.

| Step | What it does | Command behind it |
|---|---|---|
| welcome | Asks whether setup may look at which tools are installed. If you decline, each step looks only for the tools it needs. | |
| machine | Lists the tools found, with versions only, and names the ones not found. | |
| database | Offers Docker, Homebrew PostgreSQL (macOS with Homebrew), a PostgreSQL you already run, or memory only, depending on what is installed. Saves the selected storage mode transactionally and reconciles an already running owned server. Shows a warning before restarting or losing trial captures. | `mike setup --mode` |
| service | Installs the background service. It does nothing while a developer stack is running. | `mike service` |
| claude | Five parts: registers `mike-mcp` with Claude Code and counts its tools; chooses how Claude Code reaches mike; writes a shell function; installs the mods; chooses folder trust. | `mike claude-code` |
| features | Watch only, recommended (recall, lessons and reminders) or everything, starting on recommended. | `mike features` |
| models | Only for the background jobs the chosen features run. One provider for every job, or a provider and a model for each job, from the providers this machine can use (the Claude login, the Codex login, an OpenRouter key). Shows the OpenRouter spend limits, saves what you pick through the running proxy, reads it back, makes one small test call per Claude or Codex provider, checks the OpenRouter key the proxy holds, and shows the proxy's model-health rows. | `mike setup models` |
| embeddings (the step is labelled "search") | If Ollama is on the machine: pulls `nomic-embed-text` (skipped when it is there), sets `embedding.url` to `http://127.0.0.1:11434/v1`, `embedding.model` and `embedding.enabled` through the running proxy, fetches one embedding as the check (mike stores vectors of 768 numbers) and shows the proxy's search-by-meaning row. If Ollama is missing: installs it with Homebrew after a yes, and on Linux prints the install command because it needs root. It also takes the address of another embedding server, or leaves search by meaning off, in which case recall uses text matching. | `ollama pull` |
| terminal | Lists only the backends whose tools are on this machine: herdr, tmux, the Hyprland terminal (when `hyprctl` and `xdg-terminal-exec` are found) and none. The current one comes first and herdr is recommended, with `brew install herdr` offered when it is missing. The chosen backend is checked live by opening and closing a scratch workspace and is saved only when that passes. | `mike spawn doctor` |
| ready | Shows the `mike doctor` checks as a checklist, with the fix for each warning, says how to start Claude Code (`mike claude`, or the shell function if one was written), then waits for the first request to reach the proxy and shows it (id, method, path, model, status, session). `c` stops waiting, `r` checks again, `b` opens the web UI, `t` opens its tour (the home page with `?tour=1`), and Enter starts `mike claude`. | `mike doctor` |

The wizard writes to the file selected with `--config`, or the per-user config file,
`~/.config/mike/config.toml` (`$XDG_CONFIG_HOME/mike/config.toml` when set), wherever you started
it from (see [`CONFIGURATION.md`](CONFIGURATION.md)). The file is created readable by you only.
For the OpenRouter key, type the name of an environment variable that holds it, such as
`OPENROUTER_API_KEY`, and the wizard stores only that name; if you paste the key itself, it is
stored in that owner-only file in plaintext.

`mike --version` prints the version and git commit, and `mike doctor` checks the whole install
without changing anything and names the setup step that fixes each problem.

## Connecting Claude Code

The `claude` step of the wizard runs these commands, and each works on its own:

| Command | What it does |
|---|---|
| `mike claude-code status` | Shows the registration, the route, the shell function, the mods and the folder-trust choice. |
| `mike claude-code mcp [--no-check] [--remove]` | Adds a `mike` entry under `mcpServers` in `~/.claude.json` with the absolute path of `mike-mcp` (beside `mike` or on PATH; a Homebrew install gets the stable `$(brew --prefix)/bin` path) and `MIKE_INSPECT_URL` taken from the configuration. The file is rewritten atomically and every other key stays. Unless `--no-check` is given it then starts `mike-mcp` once, sends `initialize` and `tools/list`, and reports how many tools it listed. `--remove` takes the entry out only when its command is `mike-mcp`. |
| `mike claude-code route sidecar` or `plain` | Saves how Claude Code reaches mike as `claude_route` in the config file. `sidecar` is `mike claude` through the forwarding sidecar, which also captures Remote Control. `plain` sets `ANTHROPIC_BASE_URL` to the proxy and needs no certificate. |
| `mike claude-code shell [--name cc] [--shell zsh,bash,fish] [--remove]` | Writes a shell function between `# >>> mike >>>` and `# <<< mike <<<` in the startup file of each chosen shell (default: each of zsh, bash and fish that is installed), and replaces the block in place when run again. With the sidecar route the function runs `mike claude`; with the plain route it sets `ANTHROPIC_BASE_URL`. A name that is already a command on PATH is refused (the default is `cc`, or `mcc` when `cc` is taken). |
| `mike claude-code mods [--with-wiretap] [--remove]` | Sets `CLAUDE_CODE_PLUGIN_DIRS` in the `env` section of `~/.claude/settings.json` to the mods in the package's `share/mike/mods` folder (a checkout uses its own `mods/` folder), keeps every other setting and folder, and records what it added. `wiretap`, which records every prompt and tool call, is installed only with `--with-wiretap`. |
| `mike claude-code trust on` or `off` | Saves `launch_auto_trust`. With `on`, `mike claude` writes the trust answer for the folder it starts in into `~/.claude.json`. Without a saved choice, trust is off, so Claude Code asks about each new folder itself. |

`mike uninstall` removes the MCP entry, the marked shell blocks, the plugin folders it added and
the background services (`mike service uninstall`), and keeps the database, its volume and the
config file. `mike uninstall --purge` also stops the database container and then asks, one
question each and defaulting to no, whether to delete the Docker volume and the config file. It
removes only what mike wrote: an MCP entry named `mike` that does not start `mike-mcp`, a shell
file without the markers and plugin folders mike did not add are left alone.

## Running in the background

`mike service install` keeps mike running without a terminal: it starts at login and starts
again after a crash. It sets up two services, the proxy (`mike serve`, which also serves the
inspect API and the web UI) and the forwarding sidecar (`mike forward serve`), both pointing at
the `mike` binary you ran the command with. For a Homebrew install the path written is the
stable `$(brew --prefix)/bin/mike` link, not the versioned `Cellar` folder, so `brew upgrade`
keeps working; run `mike service restart` afterwards to pick up the new build.

| System | What is written | Names |
|---|---|---|
| macOS | launchd user agents in `~/Library/LaunchAgents`, loaded with `launchctl bootstrap gui/<uid>` | `ai.temet.mike.proxy`, `ai.temet.mike.forward` |
| Linux | systemd user units in `~/.config/systemd/user`, enabled with `systemctl --user enable --now` | `mike.service`, `mike-forward.service` |

Settings come from the per-user config file (see [`CONFIGURATION.md`](CONFIGURATION.md)), not
from a checkout's `.env`, and the services run in `~/.local/state/mike` (or `$XDG_STATE_HOME/mike`),
where their output goes to `logs/proxy.log` and `logs/sidecar.log`. Their `PATH` holds
`~/.local/bin`, the mise shims, the Homebrew folders and the folders that held `claude` and
`codex` when you installed, so the background-model jobs can start them.

| Command | What it does |
|---|---|
| `mike service install [--dry-run]` | Writes the two files, starts both services, then waits until the inspect API reports this binary's build and the sidecar port answers, and prints what it checked. Running it again changes nothing unless the files would differ |
| `mike service status` | Shows for each service whether it is installed, loaded and running, with the process id, then repeats the health check. Exits 1 when either service is down |
| `mike service restart [--dry-run]` | Restarts both services and repeats the health check |
| `mike service uninstall [--dry-run]` | Stops both services and removes the files, only if they begin with the comment `Managed by mike service install`. A file you wrote yourself at the same path is left alone. The log files are kept |

`--dry-run` prints the files and the commands without writing or running anything. mike refuses
to install while a developer stack is running: any `mike-stack-*` systemd unit that is active,
or any program already answering on the proxy, inspect, web UI or sidecar address. Nothing is
written in that case. While the proxy runs as an installed service, `mike claude` starts that
service when it is not answering and waits for it, and never starts a second proxy beside it.

On a machine where the service is installed, `./scripts/start-proxy.sh` and `./scripts/deploy-proxy.sh`
from a checkout deploy through it. They build mike, check which database migrations are pending,
and start a trial copy of the new build on spare ports (a smoke boot) using the service's own
configuration, not the checkout's `.env`. Only then do they copy the binary over the program the
service file names and restart the proxy service. The deploy counts the new build as live only
when `/api/health` reports a boot generation, an identifier that changes each time the process
starts, different from the one that was answering before. If that has not happened within 60
seconds (`--health-timeout` changes this), it puts the previous binary back and restarts the proxy
again. The sidecar keeps traffic flowing to Anthropic during that restart, and restarting it breaks
every stream open through it, so the deploy restarts it only when it is down or the code in
`crates/proxy-forward` changed, and only after the proxy is healthy. The web UI on port 9990 is the
service's own, so `start-proxy.sh` does not start the development web server (vite) beside it. On
macOS `start-proxy.sh` also keeps the prompt assistant running as the launchd agent
`ai.temet.mike.assistant`.

`./scripts/stop-proxy.sh` stops the services through launchd or systemd, and they stay stopped until
`./scripts/start-proxy.sh`, `mike service restart` or the next login. `start-debug.sh`,
`debug-stack-deps.sh` and `start-and-monitor.sh` bind mike's own ports, so they refuse to run until
you have stopped the service. `restart-frontend.sh` refuses port 9990 and works on any other
port, for example `MIKE_FRONTEND_PORT=9991 ./scripts/restart-frontend.sh`.

On Linux the user services stop when you log out unless lingering is on. mike does not run
`sudo`; run `sudo loginctl enable-linger $USER` once if you want them to keep running.
`mike doctor` has a "background service" check that names `mike service install` when the
services are missing.

## What's running

| Port | Process | Purpose |
|---|---|---|
| `:9988` | `mike` (proxy bind) | Claude Code's `ANTHROPIC_BASE_URL` target; forwards to `api.anthropic.com` |
| `:9989` | `mike` (inspect bind) | REST + WebSocket API backing the web UI and the MCP bridge. No authentication |
| `:9990` | `mike` (SPA bind), or `vite dev` | The web UI. mike serves the SPA embedded in the binary *and* the whole `/api/*` surface here. `start-proxy.sh` sets `MIKE_SPA_ENABLED=0` and hands the port to `vite dev` for hot reload |
| `:9987` | `proxy-forward` | HTTPS MITM sidecar with its own generated CA, started by `start-proxy.sh` (or `mike forward serve` under `mike service install`). Captures Claude Code's Remote Control WebSocket channel and fails over to Anthropic while the proxy restarts |
| `:6006` | Storybook | Component sandbox; development checkout only, started with `systemctl --user start mike-stack-storybook`, not by `start-proxy.sh` |

Three workspace binaries: `mike` (proxy, inspect, SPA, TUI, CLI, and the sidecar as `mike forward`),
`mike-mcp` (MCP stdio bridge), and `proxy-forward` (the same sidecar as a standalone binary for
the development stack). An installed `mike` needs no second binary: `mike forward` runs the sidecar
with its bind, upstream and health URL taken from the mike configuration (flags and
`MIKE_FORWARD_*` variables still win), `mike forward export-ca` prints its CA certificate, and
`mike forward ca-path` prints where `mike claude` and `mike spawn` look for that certificate.
Module layout and rationale: [`ARCHITECTURE.md`](ARCHITECTURE.md).

## Installer script (alternative to the manual quick start)

`scripts/install.sh` automates most of the steps above in one run. It's optional — everything
it does can also be done by hand — but it's the fastest path on a fresh clone, and it's safe to
re-run at any time.

```bash
git clone <this-repo> proxy
cd proxy
./scripts/install.sh
```

### What it walks through

1. **Prerequisite checks** — rust, cargo, node, pnpm are present.
2. **Rust workspace build** — `mike` + `mike-mcp`.
3. **Frontend dependency install** — `pnpm install` under `frontend/`.
4. **Storage mode choice** (interactive prompt, or via `--non-interactive`):
   - `in-memory` — no database, fastest, captured traffic is lost on restart.
   - `external` — you supply an existing PostgreSQL (local, LAN, or managed cloud) via
     `MIKE_DATABASE_URL`.

   The legacy source installer does not own installed Docker provisioning. Use
   `mike database provision` and the selected private config for the installed product;
   the development wrapper requires `scripts/local-db/up.sh --config PRIVATE_CONFIG`.
5. **Local bootstrap env update** in `.env` at the repo root (see below). An optional ports
   prompt here also lets you change the proxy/inspect/frontend ports from the defaults
   (`9988`/`9989`/`9990`).
6. **MCP bridge config snippet** for `~/.claude.json` — printed to the terminal, not written
   to disk. See [`MCP_BRIDGE.md`](MCP_BRIDGE.md) for the full registration steps.

### The `.env` bootstrap file

The installer writes a local `.env` file for convenience. Boot configuration comes from
defaults, then `MIKE_*` environment variables, then CLI flags; runtime tuning lives in
`/settings`. See [`CONFIGURATION.md`](CONFIGURATION.md). The installer only ever touches five
bootstrap keys:

- `MIKE_PERSIST_ENABLED` (`persist_enabled`)
- `MIKE_DATABASE_URL` (`database_url`)
- `MIKE_LISTEN_ADDR` (`proxy_listen_addr`)
- `MIKE_INSPECT_ADDR` (`inspect_listen_addr`)
- `MIKE_FRONTEND_PORT` (`frontend_port`)

`MIKE_FRONTEND_PORT` is read by `vite dev` and the launcher scripts, not by the Rust binary.
The port mike's own SPA bind listens on is `MIKE_SPA_BIND` / `--spa-bind`; set both if you
move the UI off `:9990`.

Other lines, comments, and hand-edited keys in `.env` are left alone.

**Diff-prompt behavior:** before writing, the installer diffs the desired values against
whatever's already in `.env`. A key that would be *added* fresh (currently unset) applies with
a plain confirm prompt (`Apply these changes? [Y/n]`, default yes) in interactive mode. A key
that would *overwrite* an existing value — a genuine conflict — gets a louder warning naming
the affected keys and an explicit `Replace the current values with the new ones? [y/N]`
prompt (default no) before proceeding.

**Non-interactive exit-code-2 behavior:** running with `--non-interactive` and hitting a
conflict (an existing `.env` value that would be overwritten) is treated as unsafe to resolve
automatically — the installer prints `bootstrap env has conflicting existing values for: <keys>`
and exits with status **2** without touching the file. Re-run interactively to approve the
change, or edit `.env` by hand and re-run.

Re-running with inputs that already match the current `.env` is a no-op either way: the
installer reports "bootstrap env already matches the selected configuration — no changes
needed" and exits 0.

### Flags

| Flag | Effect |
|---|---|
| `--release` | Builds the Rust workspace in release mode instead of debug. |
| `--non-interactive <in-memory\|external>` | Skips all prompts; picks the given storage mode. For `external`, `MIKE_DATABASE_URL` must already be set in the environment before running — this mode does not prompt for it. |
| `--migrate-env` | Deprecated no-op. Bootstrap values now live directly in `.env`; this flag performs no reads, writes, or migration — it just prints a short explanation and exits 0. Kept only for old muscle-memory/scripts. |
| `-h`, `--help` | Prints the full usage block and exits 0. |

## Next steps

- [`SECURITY.md`](SECURITY.md) — the trust boundary, and how to run with nothing enabled
- [`FEATURES.md`](FEATURES.md) — the web UI, and every optional feature with its default
- [`CONFIGURATION.md`](CONFIGURATION.md) — boot precedence and every `MIKE_*` variable
- [`MCP_BRIDGE.md`](MCP_BRIDGE.md) — registering and using the `mike-mcp` bridge
- [`TROUBLESHOOTING.md`](TROUBLESHOOTING.md) — build, toolchain and runtime failures
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — design rationale and decision log
