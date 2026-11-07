# When something looks wrong

This page is for the moment the `docs` session from the guide's fictional example does not show up, a feature does nothing, or a command refuses. It lists what you see, why, and what to do, using mike's own messages.

Start with `mike doctor`. It checks your configuration, database, ports, Claude Code, the MCP companion and more, changes nothing, and names the `mike setup <step>` that fixes each problem. `mike doctor --json` prints the same findings for a script. The rest of this page is by symptom.

## Where to look

| What | Where |
|---|---|
| Is mike up, and can it reach the database? | <http://localhost:9989/api/health>. Read `persistence`: `reachable: false` means captures are not being saved. |
| Foreground output | The terminal running `mike serve`. |
| Background service logs | `logs/proxy.log` and `logs/sidecar.log` in the state folder (`~/.local/state/mike`; `mike config path --state` prints it). |
| What settings mike started with | `mike print-config`, and `mike --config <FILE> print-config --paths` for the folders. |
| Which release you run | `mike --version` and `mike-mcp --version`. They should match. |

## Common problems

| You see | What to do |
|---|---|
| No session appears after a prompt | See [No session appears](#no-session-appears). |
| History is gone after a restart | See [History disappeared](#history-disappeared). |
| `address already in use` when mike starts | See [A port is taken](#a-port-is-taken). |
| `mike setup --mode ...` says to repeat with `--accept-trial-loss` | Add the flag. It is required with every `--mode`, because switching storage can restart mike and drop trial captures. |
| `listener ... is not loopback` | Something set a listener to a LAN or public address. Set it back to `127.0.0.1`. mike is not built to be reached from other machines; see [Privacy](privacy.md#the-security-boundary). |
| `403` with `host not allowed` or `origin not allowed` | See [The API answers 403](#the-api-answers-403). |
| Claude Code does not show the `mike` tools | See [Claude Code cannot see the MCP tools](#claude-code-cannot-see-the-mcp-tools). |
| `mike claude` prints `sidecar not reachable` | The session is still captured; it only lacks the Remote Control channel. Run `mike service install` to start the sidecar, or choose the direct route with `mike claude-code route plain`. The message `no CA certificate found; set MIKE_LAUNCH_CA_CERT` means the sidecar has not generated its certificate yet; `mike forward export-ca` creates one. |
| A feature is on but nothing happens | See [A feature does nothing](#a-feature-does-nothing). |
| A setting did not take effect | Settings marked *Applies to new sessions* leave running sessions alone. Listener, database and folder settings apply only after mike restarts. |
| A command says the database history does not match the program | See [The program and the database disagree](#the-program-and-the-database-disagree). |
| `mike claude-code shell` refuses a name | The name is already a command on your path, a shell word, or not made of letters, digits, `_` and `-`. Pass another with `--name`. |
| The background services stop when you log out (Linux) | Run `sudo loginctl enable-linger $USER` once. mike never runs `sudo`. |
| `mike service install` refuses | A development stack, or another program, is already answering on one of mike's ports. Stop it first. |
| A request hangs | Anthropic is often rate-limiting. Open the request in the web UI and check its status; mike passes `429` and `529` answers and their retry headers through unchanged. |
| `mike: command not found` | Check `brew --prefix` is on your path, or for mise run `mise use -g github:temet-ai/mike@<version>` again and open a new shell. |
| mike will not run on Linux, citing the C library | mike needs glibc 2.35 or newer, on x86_64 or ARM64. |

## No session appears

1. **Check mike is running and saving.** Open <http://localhost:9989/api/health>. A network error means mike is not up. `persistence` with `reachable: false` means the database is down; see [The database will not start](#the-database-will-not-start).
2. **Check Claude Code is going through mike.** The easiest way is `mike claude` from your project folder. If you start Claude Code yourself, use `ANTHROPIC_BASE_URL=http://localhost:9988 claude` and leave `/v1` off the address.
3. **Send a real prompt.** mike records a request only when Claude Code sends its session identifier, a valid UUID in the `x-claude-code-session-id` header. Normal prompts have one. A version check, a health request or a client that does not send the header is forwarded to Anthropic but not recorded, and mike logs a warning saying so. A client that is not Claude Code needs to send a steady UUID per conversation in that header.
4. **Check the ports.** If you changed the proxy port, `ANTHROPIC_BASE_URL` must use it. `mike print-config` shows it.
5. **Reload the page.** A project list that shows an error is not an empty database; retry it.

## History disappeared

- **You ran `trial` storage.** Trial keeps captures in memory and loses them on restart. Run `mike setup` and choose `docker` or `existing`. Moving off trial does not carry old captures over.
- **You started mike with a different configuration.** Each configuration file has its own database address and folders. Start it with the same `--config` as before. `mike print-config` shows the database it will use, with the password hidden.
- **A variable overrode the file.** `MIKE_DATABASE_URL` or `MIKE_PERSIST_ENABLED=0` in your shell beats what is saved. `mike print-config` lists the `MIKE_*` variables it saw.
- **Sessions older than 365 days are deleted** by the retention sweep.
- **The database container was removed together with its volume.** Restore from a [backup](backup-and-recovery.md).

## The database will not start

| Problem | What to do |
|---|---|
| Docker is missing or not running | Install Docker (Docker Desktop, OrbStack and Colima work) and start it, or choose `existing` and bring your own PostgreSQL. Remote Docker contexts are refused; mike needs a local Docker socket. |
| Port 6666 is taken | `mike database provision --port 6700`, or `mike setup --mode docker --port 6700 --accept-trial-loss`. |
| The container exists but is stopped | `mike database provision` starts it again with the same data. |
| The first build is slow | The first run builds the database image, which downloads packages. Later runs reuse it. |
| `mike database check` reports a missing extension or the wrong version | mike needs exactly PostgreSQL 19 with the `vector`, `pgcrypto` and `pg_trgm` extensions. For your own database, install them or run mike's first start as a user allowed to create them. |
| The config folder is refused | mike requires that the folder holding your configuration file can be entered only by you (mode `0700`). Fix the permissions rather than moving the file. |

`mike database check` reads the database and changes nothing. Only `mike serve` applies database changes.

## A port is taken

mike uses 9988 (capture), 9989 (local API), 9990 (web UI) and, for the sidecar, 9987. Find the program holding one, for example with `ss -ltnp | grep 9988` on Linux or `lsof -i :9988` on macOS. Stop it, or move mike with `--proxy-listen-addr`, `--inspect-listen-addr` or `--spa-bind` (or the variables in [Settings](settings.md)). If you move the API, tell the companion with `MIKE_INSPECT_URL`, and point Claude Code at the new capture port.

## The API answers 403

The local API only answers requests whose `Host` is `localhost` or a loopback address, and refuses cross-site browser requests. A `host not allowed` answer means something reached it by another name: a LAN address, a computer name, a tunnel. An `origin not allowed` answer means a page on another origin, or a `file://` page, called it. Use `http://localhost:9990` or `http://127.0.0.1:9990`. Adding names to `MIKE_INSPECT_ALLOWED_HOSTS` removes the check without adding a login, so only do that if you understand [what it opens](privacy.md#the-security-boundary).

## Claude Code cannot see the MCP tools

1. Make sure mike is running; the companion talks to its API.
2. Run `mike mcp probe`. It starts `mike-mcp` and checks that it connects. `mike claude-code status` shows whether an entry exists in `~/.claude.json`.
3. Register it if it is missing: `mike claude-code mcp`, or `mike mcp register`.
4. If you changed the API port, set `MIKE_INSPECT_URL` for the companion to match.
5. Restart Claude Code. It reads its MCP entries when it starts, and also after you upgrade `mike-mcp`.
6. Make sure `mike` and `mike-mcp` come from the same release. A mismatch can look like a connection or schema error.

[MCP](mcp.md) covers what the tools do.

## A feature does nothing

Open <http://localhost:9990/settings>. The capability panel at the top names what a feature is waiting for.

- **Features that call a model need a provider.** With the default models, that is an OpenRouter key in `MIKE_STEERER_OPENROUTER_API_KEY` or `OPENROUTER_API_KEY` in the environment mike starts with, or a model set to `claude/...` or `codex/...` with that tool signed in. Without one, the feature is listed as inert and sends nothing. Run `mike setup models`.
- **Embedding needs a server.** Search by meaning uses the address in the Embedding settings. Without it, recall falls back to plain text search.
- **A service started in the background may not see your shell.** Variables exported in a terminal are not there for `mike service`. Put the setting in the configuration file, or restart the service after you set them where it can read them.
- **The word list is empty.** The forbidden-word intercept does nothing until you add at least one word.
- **Session features apply to new sessions.** Start a new Claude Code session.

## The program and the database disagree

mike refuses to run most commands when the database has changes this version does not know, or when its record of past changes differs from the program's. `mike migrate-check` says which. Use the release that wrote the data, or the newer one; only `mike serve` of a version that knows the database applies its changes. Never edit the database's record of changes by hand. If an upgrade went wrong, restore your [backup](backup-and-recovery.md) into a fresh copy with the release that made it.

## Report a problem

Open an issue at <https://github.com/temet-ai/mike/issues> and include:

- the output of `mike --version` and `mike-mcp --version`;
- your operating system and architecture, and how you installed (Homebrew, mise or the archive);
- what you ran and what you expected;
- the output of `mike doctor`, and the last lines of the log.

Read everything first. Logs and `mike doctor --support-bundle` hide credentials, but they can name your folders and projects. Never attach a database dump, a backup, your configuration file or captured conversation text.
