# What mike stores and sends

mike keeps a full copy of what you send to Claude Code and what comes back: your prompts, the system prompts, the files and command output Claude Code reads, and the answers. That copy is as sensitive as the projects you work in. This page says where it lives, what leaves your machine and when, how to delete it, and what protects it.

mike has no telemetry and does not check for updates. On a fresh install the only thing that leaves your machine is the request Claude Code made, which mike passes on to Anthropic. Everything else below happens only after you turn a feature on.

You do not need to open anything to read this page. To see the facts for your own install, `mike print-config --paths` prints the folders mike uses without contacting any service, and the capability panel at the top of <http://localhost:9990/settings> names which features are on and what each is waiting for.

## One correction, followed through

The guide follows one fictional example: a session named `docs`, role `writer`, in `~/projects/site`, with the goal "Rewrite the install page". Halfway through you tell Claude: "Put Homebrew first on the install page."

- **Stored:** that sentence is in the request Claude Code sent, so it is in the database exactly as typed, with the answer that followed. It appears in the session's timeline and in search ([Reading sessions](reading-sessions.md)).
- **Sent to Anthropic:** the same request, as Claude Code made it. mike adds nothing to it unless you turned on a feature that rewrites requests.
- **Kept as a lesson, and offered to a later session:** this happens only if the memory features are on. A lesson is stored in the same database, and a later session receives it inside its own request to Anthropic. [Memory](memory.md) shows how to see and remove it.
- **Sent anywhere else:** only if you set an OpenRouter key or point mike at a hosted embedding server and turned on the feature that reads it. Then the sentence goes to that service, as the table below says.

## What is kept on your machine

| What | Where |
|---|---|
| Every captured request, response, tool call, system prompt and turn, with sessions, projects, lessons, notes, your filter rules and the settings you changed in the web UI | The PostgreSQL database. With `docker` setup this is a container named `mike-<id>-database` with a volume named `mike-<id>-data`, reachable only from your machine. With `existing` it is the database you pointed mike at. With `trial` it is held in memory and lost when mike stops. |
| Boot configuration, including the database address and any key you pasted into setup | `~/.config/mike/config.toml` (or the file you chose with `--config`), readable by you only |
| Machine identity, the managed database's files and generated password, `service-token.<port>`, logs | `~/.local/state/mike` (or `$XDG_STATE_HOME/mike`, or a `.mike-<id>` folder beside a `--config` file) |
| Data folder | `~/.local/share/mike` on Linux, `~/Library/Application Support/mike` on macOS |
| The forwarding sidecar's certificate authority, only if you use the sidecar | The sidecar's data folder; `mike forward ca-path` prints the certificate file inside it. The key is readable by you only. |
| Shell function and mods | Your shell's startup file, between `mike` markers; `~/.claude/settings.json`; the MCP entry in `~/.claude.json`. Only if you let setup write them. |

What is stored and what is not:

- **Bodies are stored whole.** A request or response is kept exactly as it crossed the wire. If a session reads a file that holds a secret, the secret is in the conversation, goes to Anthropic, and is stored with it. mike does not filter bodies.
- **Credentials in headers are not stored.** The `authorization`, `proxy-authorization`, `x-api-key`, `cookie` and `set-cookie` headers, and any header whose name ends in `token` or contains `api-key`, `secret`, `password` or `credential`, are stored as `[redacted]`. The real value is still passed on to Anthropic, as it must be.
- **The capture holds no Anthropic credential of its own.** It forwards the one Claude Code sends.
- **Keys you give mike are never written to the database.** An OpenRouter key lives in your environment or in your configuration file. A setting whose name contains `key`, `secret`, `token`, `password` or `credential` is refused by the settings store.
- **A copy of the original request is kept even when a feature rewrites what is forwarded.** What mike captured is what Claude Code sent, not what mike passed on.
- **Old sessions are deleted.** A sweep runs every hour and removes sessions older than 365 days.

## What leaves your machine

On a fresh install, only the first row happens.

| To | What | When |
|---|---|---|
| Anthropic (`https://api.anthropic.com`) | The request Claude Code sent, with its own credential, after any rewrites you turned on | Every request that goes through mike |
| `openrouter.ai` | Captured conversation text: prompts, answers, tool-call summaries and stored lessons | Only when an OpenRouter key is set **and** a feature that uses it is on: `steerer`, `judge_worker`, `session_judge`, `turn_guard`, the correction check or lesson comparison. All are off by default. A job set to a `claude/` or `codex/` model uses that tool's own sign-in and sends its text to that provider instead. |
| The address in `embedding.url` | Text to embed: turns, lessons, recall queries | Only when `embedding.enabled` is on. The default address is on your own machine. mike refuses a non-local address unless `semantic.allow_remote_embedding` is also on. |
| `claude` or `codex` on your machine | Prompts you write to the prompt assistant, and the small test call that `mike setup models` makes | Only when you use them. They run under that tool's own sign-in. |
| Your OpenTelemetry collector | Timing spans with ids, sizes and counts | Only when `otel_endpoint` is set. It is unset by default. |
| Docker Hub, Debian package mirrors and GitHub | The download and build of the managed database image | Once, when `mike database provision` or `docker` setup first builds it |

Every outbound model call's full body is recorded in the database, so you can read exactly what was sent. A few evaluation commands, such as `mike eval run` and `mike verifier-replay --execute`, can also send past turns to OpenRouter; they send nothing unless you run them with a key set.

Features that rewrite requests change what Anthropic receives, not what leaves your machine for anyone else. [Settings](settings.md) lists the switches and their defaults. Each feature's page says what it sends: [Memory](memory.md), [Steering](steering.md), [Judge](judge.md), [Filters](filters.md) and [Compression](compression.md).

### The forwarding sidecar

The optional sidecar sits on `127.0.0.1:9987`, opens HTTPS connections on Claude Code's behalf and signs them with a certificate authority it generates for itself. A client that trusts that authority trusts it for every site, so treat the sidecar's certificate file like a password and do not add it to your system's certificate store. Keep the sidecar on `127.0.0.1`: bound to any other address, it would let other machines reach mike's local API as if they were you.

## Delete what mike stored

Stop mike first. Nothing below is undone by reinstalling.

1. **Delete the captures.** With `docker` setup, list the database and its volume, then remove them:

   ```sh
   docker ps -a --filter label=io.temet.mike.managed-database
   docker volume ls --filter label=io.temet.mike.managed-database
   docker rm -f <container name>
   docker volume rm <volume name>
   ```

   With `existing`, drop the mike schemas or the database in PostgreSQL. With `trial`, stopping mike already discarded them.
2. **Delete the configuration and state.** Remove the folders in the table above, or the `.mike-<id>` folder beside a custom configuration file. This removes the configuration file, the managed database's password, `service-token.<port>` and the logs.
3. **Delete the sidecar's certificate authority,** if you used the sidecar: the folder that holds the file `mike forward ca-path` prints.
4. **Delete backups.** A backup holds complete conversations and the configuration. [Back up and recover](backup-and-recovery.md) says where yours are.
5. **Remove the connections** with `mike uninstall`, which takes out the MCP entry, shell function, mods and background services it added. It keeps the database and your configuration file.

Deleting a session from mike does not recall what Anthropic or any other service already received.

## The security boundary

**mike's local API has no authentication.** The API on port 9989, and the same API on the web UI's port 9990, answer any program on the machine. Anything that can reach them can:

- read every captured request, response, system prompt and tool result;
- read and change every setting, and turn on any feature, including ones that rewrite requests or send text to other services;
- create and delete filter rules, which rewrite the requests mike forwards;
- start Claude Code sessions on the machine, in a folder and with arguments it chooses.

There is no read-only mode and no per-route permission, and mike has no login screen.

What this means for you:

- **Use mike on a computer only you use.** Another account on the same machine can read your captures.
- **Keep it on loopback.** By default all three listeners (9988, 9989, 9990) are on `127.0.0.1`. mike refuses to start with a listener on another address unless you set an environment variable whose name says it is unsupported. Do not set it, and do not put the ports behind a tunnel, a reverse proxy or a LAN address.
- **Do not add host names to `MIKE_INSPECT_ALLOWED_HOSTS`** to share an instance. It removes a check; it adds no login.
- **The capture port has no check at all.** Anything that can reach port 9988 can send traffic through mike and have it recorded.
- **A copy on another machine is a copy.** Database dumps, backups and `mike doctor --support-bundle` output hold or describe your conversations. Do not post them in an issue.

mike does check one thing: a web page you open cannot use your browser to reach the API. The API rejects requests whose `Host` is not `localhost` or a loopback address, and cross-site browser requests. That stops a hostile page, not a program on your machine.

The complete trust boundary is in [SECURITY](https://github.com/temet-ai/mike/blob/main/docs/SECURITY.md), and the same storage and network facts as a short list are in [WHAT_MIKE_STORES_AND_SENDS](https://github.com/temet-ai/mike/blob/main/docs/WHAT_MIKE_STORES_AND_SENDS.md).

## When storage is off or unavailable

| What you see | What it means |
|---|---|
| The sessions list stays empty after a prompt, and `persistence` in <http://localhost:9989/api/health> says `reachable: false` | The database is down, so nothing is being saved. [Troubleshooting](troubleshooting.md#the-database-will-not-start) |
| Captures vanish after a restart | You chose `trial`, which keeps them in memory only |
| A feature that sends text shows as inert in the capability panel | It has no provider or key, so it sends nothing |
