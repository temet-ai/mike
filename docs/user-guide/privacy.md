# What mike stores and sends

mike keeps a full copy of what you send to Claude Code and what comes back: your prompts, the system prompts, the files and command output Claude Code reads, and the answers. That copy is as sensitive as the projects you work in. This page says where it lives, what leaves your machine and when, how to delete it, and what protects it.

mike has no telemetry and does not check for updates. On a fresh install the only thing that leaves your machine is the request Claude Code made, which mike passes on to Anthropic. Everything else below happens only after you turn a feature on.

You do not need to open anything to read this page. To see the facts for your own install, `mike print-config --paths` prints the folders mike uses without contacting any service, and the capability panel at the top of <http://localhost:9990/settings> names which features are on and what each is waiting for.

## Traffic without a session id

In alpha.5, the ordinary proxy path forwards requests without a usable canonical session
header without capturing them. Source containing PR #185 changes that behavior: with
persistence available, `requests.sessionless_requests` stores the request body, method,
path, query, user agent and captured headers, plus response metadata and eligible response
bytes. This separate store is not a session timeline. Missing history is not a privacy opt-out.
The recognized Remote Control route has its own path; do not generalize this rule to every
sidecar, WebSocket or runtime-observation route.

The header is `x-claude-code-session-id`; missing, malformed and non-UTF-8 values take the
sessionless path. Per-session mutation stages do not run there. Request bodies have already
passed the configured request-size limit. Response bytes are retained as observed up to
`max_response_body_bytes`: exceeding a positive cap discards the stored response body whole,
not the forwarded response; zero means no cap. Error or aborted streams can retain only
observed bytes and a status, not a complete response. Recording is asynchronous; disabled
persistence, failed writes or interruption can leave no durable row.

Captured request and response headers use name-based masking: recognized credential/cookie
headers, names containing credential markers and names ending in `token` become `[redacted]`.
Other valid header values remain; non-UTF-8 values become empty strings. This is not general
secret removal from bodies, URLs, query strings or custom header names. In particular the
session-id header is not masked by this rule, and malformed session-id values can also enter
warning logs. Do not put secrets there. Protect database copies and logs even when no session
appears in the UI. These statements describe checked source, not proof of your installed build.

## One correction, followed through

The guide follows one fictional example: a session named `docs`, role `writer`, in `~/projects/site`, with the goal "Rewrite the install page". Halfway through you tell Claude: "Put Homebrew first on the install page."

- **Stored:** that sentence is in the request Claude Code sent, so it is in the database exactly as typed, with the answer that followed. It appears in the session's timeline and in search ([Reading sessions](reading-sessions.md)).
- **Sent to Anthropic:** the same request, as Claude Code made it. mike adds nothing to it unless you turned on a feature that rewrites requests.
- **Kept as a lesson:** you can explicitly record it with the MCP companion, or have Claude emit a lesson tag that the default harvester reads. Storing a lesson and automatically delivering it are separate actions. When delivery is enabled and the lesson is selected, its text becomes part of a later request to Anthropic. [Memory](memory.md) shows how to verify both steps and remove a lesson.
- **Sent to another model service:** a feature you enabled can send captured text to its configured provider. That can be OpenRouter or a job run through your signed-in `claude` or `codex` tool; no OpenRouter key is needed for those two routes. Enabled embedding and reranking features send text to their configured servers. The table below lists these destinations.

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

## Code scans and feedback

In post-alpha.5 builds with code findings, scanning recorded edits is local, but delivery
is not. `code_findings` sends paths, line numbers, rule messages and suggested fixes to
Claude/Anthropic in a later request. `code_findings_push` also permits proactive delivery
over a live Remote Control channel, asking the session to fix the code or explain it.
This can initiate further work and model usage; it is not merely a local notification.
The recommended preset enables this path in these builds. For local inspection only,
keep `code_findings` off; [Settings](settings.md#choose-how-code-findings-reach-a-session)
explains the independent recording, scanning and delivery switches.

## Knowledge sources and standing lessons

Post-alpha.5 builds with configurable knowledge kinds can make selected database columns
searchable. Preview shows real source content; do not share its output without reviewing
it. If embedding or reranking is enabled, the selected text can go to those configured
servers. Search results returned to Claude become part of later model requests. A local
source table is not a guarantee that its text stays local.

Current post-alpha.5 source builds also automatically index selected files in the Claude
home of the account running mike: agents, skills, rules, agent memory and, with the rules
source present, global `CLAUDE.md`. Registered checkout instructions are indexed too.
This needs a database but does not require embeddings or a dedicated opt-in toggle.
`CLAUDE_CONFIG_DIR` selects that home; otherwise it is `$HOME/.claude`.

Machine-wide indexed sources can appear in searches for any project sharing the database,
including searches from another machine. Machine attribution is not access control.
Indexed text can subsequently go to embedding/reranking services when enabled, or into
Claude's conversation when retrieved. There is no dedicated scanner-off switch here;
disabling embeddings does not stop indexing, and a missing source directory retains its
previous index rather than erasing it. See [Memory](memory.md#machine-wide-claude-sources)
for scope and version limits. Do not use moving the source files as proof that stored
copies are gone.

In builds with lesson routing, promotion can write standing lessons automatically to
`.claude/rules/mike-lessons.md` in a registered project checkout. That file can be read by
Claude and can be included in a Git commit if you stage it. Review the generated content
before sharing the repository. Project rules and files already loaded into a session are
not recalled by disabling proxy injection. See [Memory](memory.md) for routes and how to
check whether a lesson was already present instead of newly injected.

## Wright evidence and external follow-ups

Wright is off by default. In post-alpha.5 builds with its follow-up worker, enabling it can
send investigation evidence to the model provider and publish GitHub issues in the configured
repository. An issue includes the follow-up's title and body, check name, owning code area,
and links to problem and finding records. There is no general sensitive-data redaction in
that issue formatter. Treat text entering those records as potentially publishable; do not
assume captured secrets are removed or every issue waits for your approval.

The problem links use a localhost address. They neither make local evidence accessible to a
remote reader nor undo disclosure of text already copied into the issue body. The fixer may
read additional captured evidence and use its machine's normal tools and credentials.
Check the repository's audience and the configured fixer destinations before enabling this
path. [Wright controls](settings.md#review-wright-before-enabling-it) distinguish local
inspection, model investigation, constrained internal repair and ordinary fixer sessions.

In current post-alpha.5 source, internal engineer runs also receive the Mike checkout's
existing `.claude/agents`, `.claude/rules` and `.claude/skills` directories, mounted read-only
into their private Claude home. Read-only prevents writing through those mounts; it does
not make the contents private from the model. Treat instructions and files the engineer
reads there as potential model input. These mounts come from the configured Mike checkout,
not your normal Claude home; they are separate from the machine-wide knowledge index.
Review the checkout's contents before enabling Wright. Internal investigations use a
read-only source workspace, but internal repairs get a writable repair worktree. Ordinary
follow-up fixers do not inherit those sandbox boundaries.

### SQL performance evidence can become repository content

In source containing PR #411, Wright's SQL repair path can measure representative queries
against the capture database. The engineer supplies SQL files; the executor performs the
measurement. `EXPLAIN ANALYZE` executes the query: it is not a read-only plan preview.
Transaction rollback is not a general sandbox or a guarantee against every side effect.

**Query literals must be safe for the repository's audience.** Before/after SQL inputs
are copied verbatim into `scripts/db-code-graders/perf-evidence/` and staged with the repair.
There is no redaction step in that copy. Sensitive identifiers, text or credentials embedded
in SQL can enter commit history and subsequent publication. Removing `.wright/perf` scratch
files does not remove committed evidence.

Check both the measurement database and evidence files before allowing publication.
A performance fingerprint binds evidence to a query; it does not certify privacy.

### Protect the operator credential

The pairing URL printed by `mike wright pair --open` contains an operator credential. Keep
it out of screenshots, recordings, issue bodies and shared logs. The paired browser stores
the token locally; hiding it from the address bar is not revocation. A copied token is not
made harmless by clearing one browser's site data.

## What leaves your machine

On a fresh install, only the first row happens.

| To | What | When |
|---|---|---|
| Anthropic (`https://api.anthropic.com`) | The request Claude Code sent, with its own credential, after any rewrites you turned on. History read through MCP also enters Claude Code's subsequent requests as tool results. | Every request that goes through mike |
| The provider configured for a background model job: OpenRouter, Claude or Codex | Captured conversation text appropriate to the job, such as prompts, answers, tool-call summaries and stored lessons | When the feature is enabled and its provider is available. Examples include steering, judging, the turn guard and lesson proposals. OpenRouter needs its key; jobs routed to `claude/` or `codex/` use that tool's own sign-in and send the job's text through that provider instead. |
| The address in `embedding.url` | Text to embed: turns, lessons, recall queries | When `embedding.enabled` is on; it is off in a fresh configuration. The default address is on your own machine. mike refuses a non-local address unless `semantic.allow_remote_embedding` is also on. |
| The address in `rerank.url` (after alpha.5) | Search queries and candidate passages | When `rerank.enabled` is on and a search uses the reranker. It is off by default, and its default address is `http://127.0.0.1:11435`. A server on another machine receives that text. |
| The provider reached by `claude` or `codex` | Prompts you write to the prompt assistant, and the small test call that `mike setup models` makes | Only when you use them. The local tool runs under its own sign-in; running the tool locally does not mean its model inference stays on your machine. |
| Your OpenTelemetry collector | Timing spans with ids, sizes and counts | Only when `otel_endpoint` is set. It is unset by default. |
| Docker Hub, Debian package mirrors and GitHub | The download and build of the managed database image | Once, when `mike database provision` or `docker` setup first builds it |

Every outbound model call's full body is recorded in the database, so you can read exactly what was sent. A few evaluation commands, such as `mike eval run` and `mike verifier-replay --execute`, can also send past turns to OpenRouter; they send nothing unless you run them with a key set.

Adding a lesson or reminder to a request sends that context to Anthropic. The rewrite itself does not require a separate model provider, but other features you enable can make the additional calls listed above. [Settings](settings.md) lists the switches and their defaults. Each feature's page says what it sends: [Memory](memory.md), [Steering](steering.md), [Judge](judge.md), [Filters](filters.md) and [Compression](compression.md).

### The forwarding sidecar

The optional sidecar sits on `127.0.0.1:9987`, opens HTTPS connections on Claude Code's behalf and signs them with a certificate authority it generates for itself. A client that trusts that authority accepts certificates the sidecar signs for any hostname. Protect its private signing key (`ca-key.pem`), which is separate from the certificate (`ca-cert.pem`) a client loads. Keep that trust limited to the intended client; do not add the CA to your system's certificate store. Keep the sidecar on `127.0.0.1`: bound to any other address, it would let other machines reach mike's local API as if they were you.

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

**mike has no general login or user-permission boundary.** Most of the API on port 9989,
and the same API on the web UI's port 9990, answer programs on the machine without a
credential. A few sensitive routes require tokens, including Wright's operator actions;
this does not protect the rest of the local API. Anything that can reach it can:

- read every captured request, response, system prompt and tool result;
- read and change every setting, and turn on any feature, including ones that rewrite requests or send text to other services;
- create and delete filter rules, which rewrite the requests mike forwards;
- start Claude Code sessions on the machine, in a folder and with arguments it chooses.

There is no general read-only role or multi-user permission model, and mike has no login
screen. Wright pairing is not permission to expose these ports to other machines.

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
| `persistence.enabled` is `false` in <http://localhost:9989/api/health> | No database is attached. In trial mode this is expected: captures are in memory, and `reachable: false` is not a database outage. |
| `persistence.enabled` is `true`, but `reachable` is `false` | The database probe failed. Read `reachable_state` and the log; a timeout does not prove the database process stopped. [Check storage and capture health](troubleshooting.md#check-that-history-is-being-saved). |
| Claude answers but history has gaps, even with `reachable: true` | Forwarding can continue while writes fail. Check `capture_stalled` and the top-level `unrecorded_captures` section before restarting; [Troubleshooting](troubleshooting.md#check-that-history-is-being-saved) explains the evidence and its limits. |
| Captures vanish after a restart | You chose `trial`, which keeps them in memory only |
| A feature that sends text shows as inert in the capability panel | It has no provider or key, so it sends nothing |
