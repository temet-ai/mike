# When something looks wrong

This page is for the moment the `docs` session from the guide's fictional example does not show up, a feature does nothing, or a command refuses. It lists what you see, why, and what to do, using mike's own messages.

Start with `mike doctor`. It checks your configuration, database, ports, Claude Code, the MCP companion and more, changes nothing, and names the `mike setup <step>` that fixes each problem. `mike doctor --json` prints the same findings for a script. The rest of this page is by symptom.

## Where to look

| What | Where |
|---|---|
| Is mike up, and is it saving history? | <http://localhost:9989/api/health>. Check the storage mode and write status under `persistence`; see [Check that history is being saved](#check-that-history-is-being-saved). |
| Foreground output | The terminal running `mike serve`. |
| Background service logs | `logs/proxy.log` and `logs/sidecar.log` in the state folder (`~/.local/state/mike`; `mike config path --state` prints it). |
| What settings mike started with | `mike print-config`, and `mike --config <FILE> print-config --paths` for the folders. |
| Which release you run | `mike --version` and `mike-mcp --version`. They should match. |

## Common problems

| You see | What to do |
|---|---|
| No session appears after a prompt | See [No session appears](#no-session-appears). |
| History is gone after a restart | See [History disappeared](#history-disappeared). |
| Claude answers, but turns are missing from history | See [Check that history is being saved](#check-that-history-is-being-saved). |
| `address already in use` when mike starts | See [A port is taken](#a-port-is-taken). |
| `mike setup --mode ...` says to repeat with `--accept-trial-loss` | Add the flag. It is required with every `--mode`, because switching storage can restart mike and drop trial captures. |
| `listener ... is not loopback` | Something set a listener to a LAN or public address. Set it back to `127.0.0.1`. mike is not built to be reached from other machines; see [Privacy](privacy.md#the-security-boundary). |
| `403` with `host not allowed` or `origin not allowed` | See [The API answers 403](#the-api-answers-403). |
| Claude Code does not show the `mike` tools | See [Claude Code cannot see the MCP tools](#claude-code-cannot-see-the-mcp-tools). |
| `mike claude` prints `sidecar not reachable` | The session is still captured; it only lacks the Remote Control channel. Run `mike service install` to start the sidecar, or choose the direct route with `mike claude-code route plain`. The message `no CA certificate found; set MIKE_LAUNCH_CA_CERT` means the sidecar has not generated its certificate yet; `mike forward export-ca` creates one. |
| Project search is ambiguous, or answers with unrelated material | Check [project identity and result provenance](projects.md#if-project-search-is-ambiguous-or-unrelated) before accepting a suggested UUID. |
| A remembered lesson is missing from search | See [Find a missing lesson](#find-a-missing-lesson). |
| A scheduled job is late, failing or stuck on queued | See [Check a scheduled job](#check-a-scheduled-job). |
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

1. **Check mike is running and saving.** Open <http://localhost:9989/api/health>. If it does not answer, run `mike doctor` and check the configured API address. If it answers, [check the storage and capture fields](#check-that-history-is-being-saved) below. A working proxy does not prove a request was saved.
2. **Check Claude Code is going through mike.** The easiest way is `mike claude` from your project folder. If you start Claude Code yourself, use `ANTHROPIC_BASE_URL=http://localhost:9988 claude` and leave `/v1` off the address.
3. **Send a real prompt.** Session history needs a valid UUID in `x-claude-code-session-id`. Alpha.5 forwards ordinary proxy traffic without a usable header without capture; source containing #185 stores it separately as sessionless traffic when persistence is available, not in session history. Missing headers do not produce the newer malformed-header warning. Check the running version and [sessionless privacy limits](privacy.md#traffic-without-a-session-id), rather than treating a missing timeline as no storage.
4. **Check the ports.** If you changed the proxy port, `ANTHROPIC_BASE_URL` must use it. `mike print-config` shows it.
5. **Reload the page.** A project list that shows an error is not an empty database; retry it.

## Check that history is being saved

mike keeps forwarding to Anthropic when database writes fail, so Claude can answer while
history is missing. Read <http://localhost:9989/api/health> before restarting anything:

1. **Check the storage mode.** Under `persistence`, `enabled: false` means no database is
   attached. That is expected in trial mode: captures stay in memory until mike stops.
   `reachable: false` in that mode is not a database outage. If you expected durable history,
   check `mike print-config` and [your database choice](setup.md#choose-where-history-is-kept).
2. **Check the connection.** With `enabled: true`, `reachable: false` means the database
   probe did not succeed. Read `reachable_state` and the proxy log, then use
   [The database will not start](#the-database-will-not-start). A timeout does not by itself
   mean the database process has stopped.
3. **Check writes, not only the connection.** `capture_stalled: true` means recent requests
   reached mike but saved captures have stopped advancing. This can happen even with
   `reachable: true`, for example when storage is full. Read the proxy log for the failed
   write. `latest_capture_write_at` shows the latest saved request; an old time alone is
   normal when no new requests are arriving.
4. **Look for delayed or missing requests on newer builds.** The top-level
   `unrecorded_captures` section was added after `v0.1.0-alpha.5`; that release does not
   expose this section or the associated delayed-capture retry ledger. On builds that
   include it,
   `waiting` lists requests kept in memory while mike tries to save them. `recorded_late_count`
   counts those subsequently saved. `unrecorded` lists requests mike could not record, with
   the session, send time and reason. Those lists and counts belong to the current process
   and reset on restart; an empty list after restarting does not prove nothing was lost.

On `v0.1.0-alpha.5`, use the persistence fields in steps 1–3, the proxy log and the
actual session history. An absent `unrecorded_captures` section is expected for that release,
not evidence that every request was saved.

Keep a private copy of the relevant error and timestamps before a restart. Waiting requests
are not a backup and may be lost if mike stops. Review and redact that information before
[reporting a problem](#report-a-problem); do not post captured text.

After the underlying problem is fixed, send a new prompt and find that exact turn in
[Sessions](reading-sessions.md). With persistence enabled, check that the latest capture
write time advances too. New requests being saved does not mean every missing request from
the outage was recovered.

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
2. Run `mike mcp probe --json`. It checks the currently resolved companion against the selected Mike configuration, not Claude's saved entry. Expect `handshake: true`, `backend_read: true` and `code: "MCP_READY"`. `mike claude-code status` shows the registration.
3. Register it if it is missing: `mike claude-code mcp`, or `mike mcp register`.
4. If the probe passes but Claude fails, inspect Claude's saved command and `env.MIKE_INSPECT_URL`. [After an upgrade](mcp.md#after-an-upgrade) explains stale executable paths and preserving custom arguments and environment.
5. Start a new Claude Code session after updating its entry, then ask it to call the read-only `get_stats` tool. A successful tool result verifies the registration Claude actually loaded.
6. Make sure `mike` and `mike-mcp` come from the same release. A mismatch can look like a connection or schema error.

[MCP](mcp.md) covers what the tools do.

## A feature does nothing

Open <http://localhost:9990/settings>. The capability panel at the top names what a feature is waiting for.

- **Features that call a model need a provider.** With the default models, that is an OpenRouter key in `MIKE_STEERER_OPENROUTER_API_KEY` or `OPENROUTER_API_KEY` in the environment mike starts with, or a model set to `claude/...` or `codex/...` with that tool signed in. Without one, the feature is listed as inert and sends nothing. Run `mike setup models`.
- **Embedding needs a server.** Search by meaning uses the address in the Embedding settings. Without it, recall falls back to plain text search.
- **A service started in the background may not see your shell.** Variables exported in a terminal are not there for `mike service`. Put the setting in the configuration file, or restart the service after you set them where it can read them.
- **The word list is empty.** The forbidden-word intercept does nothing until you add at least one word.
- **Session features apply to new sessions.** Start a new Claude Code session.

## Find a missing lesson

A search miss does not prove a lesson was deleted or that search regressed. First check
[the project identity](projects.md#if-project-search-is-ambiguous-or-unrelated). Then ask
the MCP companion to repeat `search_project_knowledge` with the same project and query,
`kind: "lesson"`, `include_historical: true` and `detail: "full"`. Ordinary search leaves archived, expired
and superseded lessons out. Historical search includes them as evidence; it does not
restore them or make them active instructions.

Check the full search result's status and metadata, including any retirement reason,
`superseded_by`, `contradicted_by` or `retired.by_lesson` reference. Read the lesson text
with `read_project_knowledge` before concluding that the old advice still applies. Follow a replacement reference where one
exists; trimming does not always leave a successor. Do not re-record an old lesson just
to make a probe pass. The [corrected investigation of missing lessons](https://github.com/temet-ai/mike/issues/284#issuecomment-6899689254)
found archived lessons behind a reported regression; that report is not a diagnosis of
your database.

For an older conversation rather than a lesson, remove the lesson-only kind filter. On
newer source builds, pass the known time window as `since` and `until`, not just words
such as "last Tuesday" in the query. Dates use UTC; a date-only `until` includes that
whole day, while an RFC 3339 `until` is exclusive. Read the matched source, not just its
rank: new captured material can move an old answer down. `include_historical` exists in
`v0.1.0-alpha.5`; the time-window fields described here are later additions. Check the
[installed MCP tool schema](mcp.md) rather than assuming a source feature is deployed.

## Check a scheduled job

This walkthrough describes the **Scheduled jobs** panel added after `v0.1.0-alpha.5`.
Do not expect that release to show these controls. On a build that includes them, open
**Home** and find **Scheduled jobs** before changing settings or starting another run.

1. Check the machine named above the rows. Expand the job to read **Runs on**, its
   description, schedule and latest output. **elsewhere** means another machine owns
   this job; inspect that machine's mike rather than trying to force a local run.
   **no scripts** means this installation cannot run the job here. Read the detail:
   it may lack a required script or need a mike source checkout. On source builds
   including the direct-command fallback described below, the five built-in jobs
   can launch without their wrapper scripts. A launched command may still need helper files; check the running build and the command’s output before applying that advice.
2. Read the bar as a window in which the job may **start**, not its expected duration.
   The line beneath it represents the last run. Schedules use UTC; the extra **local**
   axis uses your browser's time zone, not necessarily the server's. Expand the row
   for the schedule and full timestamps. Row order is not a dependency guarantee:
   jobs have separate workers and can overlap, even when the panel describes nightly
   learning jobs as running in order.
3. Compare **Last started**, **Last finished**, **Exit code** and **Output**. **queued**
   means a run was requested, not that it started or finished. **running** is not a
   completion receipt either. If the panel says **not refreshed since**, its retained
   rows may be stale; restore the connection and refresh before drawing conclusions
   or requesting another run.
4. For **failing**, **overdue** or **never run**, read the detail before acting. It may
   explain a failed command, a missed window while mike was stopped, or no recorded
   run yet. Check the proxy log for `scheduled_jobs` around that time. An **ok** row
   can still report an alarm with a nonzero exit code: some jobs completed their report
   but found something requiring attention. Read the alarm and output, not just the badge.

**Run now is real work, not a preview.** Depending on the job it can write project files,
change settings, run tests or call a model provider. Read the description and settings
first. If you intend those effects, press it once in the owning machine's web UI, then
wait for a new start and finish time and inspect that run's output. Acceptance alone is
not success. A refusal means you should resolve the stated cause, not retry repeatedly.
The button is unavailable while running or queued, when off, on another owner, or without
the prerequisites for that job. A manual run does not change the regular schedule.

On source builds including [the direct-command fallback](https://github.com/temet-ai/mike/pull/300),
these jobs can run without wrapper scripts: learning promotion (`mike learning promote`),
weekly learning audit (`mike learning-audit`), file-read compressor loop
(`mike compressor-loop`), lesson relation detector (`mike detect-lesson-relations`) and
decisions loop (`mike decisions loop`). A discovered checkout or package script still
wins. Otherwise the scheduler uses its own running binary, not a different `mike` on your
shell's path. Learning promotion, learning audit and compressor loop no longer need
Python; installing Python is not a universal fix. Lesson file commit, weekly usage audit
and daily memory review still need a mike source checkout and their scripts.

For **learning promotion**, successful command discovery is only the first step. The
command separately locates its helper scripts through `MIKE_SCRIPTS_ROOT`, local/package
paths or a checkout registered to this machine. If it reports that it cannot find the
Mike scripts folder, check that root rather than installing Python. On current builds,
exit **3** is a completed run with an alarm for generated-file conflicts, not an all-clear
or a generic launch failure. Read the run's JSON `conflicts` and preserve manual edits;
other updates may already have happened. [Promotion checkpoints](memory.md#promotion-started-but-did-it-finish)
explain the report location, dry-run limits and verification steps.

This is source behavior added after `v0.1.0-alpha.5`, not confirmation that a release or
your service includes it. Check the service's [running build](backup-and-recovery.md#upgrade)
and the job's actual output; a newer binary in your terminal does not update a running
service by itself.

The switch stops or resumes future scheduled runs; turning it off does not kill a job
already running. These are shared settings, not a pause local to this browser. If you
cannot establish what happened, preserve the job name, machine, timestamps and redacted
error for [a problem report](#report-a-problem). Do not attach captured text or credentials.

## The program and the database disagree

mike refuses to run most commands when the database has changes this version does not know, or when its record of past changes differs from the program's. `mike migrate-check` says which. Use the release that wrote the data, or the newer one; only `mike serve` of a version that knows the database applies its changes. Never edit the database's record of changes by hand. If an upgrade went wrong, restore your [backup](backup-and-recovery.md) into a fresh copy with the release that made it.

## Wright has not resolved a problem

This path needs a post-alpha.5 build with Wright. Read [its authority and controls](settings.md#review-wright-before-enabling-it)
before enabling workers or raising limits.

| What you see | Check before retrying |
|---|---|
| **Not in the running mike yet** | A page may be newer than the running backend. Verify the running version, not only the CLI on PATH. |
| No Wright notice reaches a session | In builds containing #415, check for a live, leased, non-fleet session with role `wright`. Browser pairing does not select it. Read pending decisions, held batches and the cause queue on `/wright`; claims happen before enqueue and not every failed delivery is retried. See [notice routing](settings.md#choose-a-session-for-wright-notices). |
| Browser remains unpaired | Private browsing or blocked local storage can prevent saving the token. Pair your own browser again; never send the pairing URL in a report. |
| No investigation started | Check the toggle, recorded limits, existing run, machine load, deploy activity and admission reason. An open problem is not a promise of an immediate run. |
| Investigation ended without establishing a cause | Read its evidence and exit reason. No finding is not proof the fault is absent. Investigation attempts are bounded, not an unlimited retry loop. |
| Follow-up stays open or filed | Read its failure reason, separate filing/handoff budgets, GitHub access, available fixer machines and their activity. An empty fixer list selects local fallback, not disabled handoff. |
| Follow-up is stuck | Inspect its attempt history and linked fixer session before raising limits or launching another. |
| Repair is batched | In source containing PR #409, inspect its daily batch. Committed is not merged, deployed or healthy. Check the UTC day boundary, fix host, active writers and gate state. |
| Shutdown interrupted a repair | Inspect the interrupted marker and recovery record, not just a failed command. Do not remove the shared daily worktree to retry. |
| SQL repair cannot measure | In source containing PR #411, check the executor error, database availability and executable SQL inputs. Wright requires exit status 0 plus a `Perf-Verdict`; unavailable measurement is not successful evidence. |
| Repair landed but the symptom remains | Merged code may not be deployed, or may not fix the check. Verify the running build and a subsequent check result. |
| Undo reports a conflict | The resource or version changed. Review current state and recorded effect; do not overwrite newer work to force an undo. |

SQL-change detection does not prove every SQL edit was measured. Current detection covers
migration SQL and selected added Rust query patterns; review changes outside those patterns
explicitly. See [SQL evidence privacy](privacy.md#sql-performance-evidence-can-become-repository-content).

Preserve problem, action, repair or follow-up ids and relevant timestamps. Redact evidence
before reporting it, and never include the pairing credential. Switching off Wright is not
an undo of external publications or cancellation of an already-started ordinary fixer.

## Changes cannot resolve a checkout

On post-alpha.5 builds containing [PR #449](https://github.com/temet-ai/mike/pull/449),
callers using the bounded repository-identity lookup wait up to three seconds for
filesystem reads on a blocking worker. The warning `file read took longer than its
limit; carrying on without it` with `what="repo identity"` means the caller continued
without that result. It does not prove the repository is absent or cancel a read
already running on the blocking worker; this is not a three-second limit for the
entire API request.

A Changes request may then report an error beginning `no checkout: pass`. Missing or
unrecorded working folders and an unrecognized repository can produce the same error.
Check the running build, session identity, owning machine and actual working folder
before changing anything. Compare the resolved Git root using the read-only commands
below, and preserve the time and redacted warning rather than deleting Git metadata
or reassigning the project.

For a Changes query without a resolved active fleet seat, an explicit nonblank
`checkout` is used before automatic discovery from the session's working folder.
Supply only a verified checkout path for the intended machine. A resolved fleet
seat's configured repository wins over that argument. Supplying the path only
bypasses this identity lookup: it does not repair filesystem access, validate the
path as a repository at this step, bypass session requirements or restore missing
change history.

The separate checkout-presence watcher uses the same three-second bound for the
filesystem-observation portion of each presence pass. If it overruns, it skips
recording presence marks and automatic location moves for that pass; it does not
mark every checkout absent. Check subsequent observations rather than treating an
unchanged location as proof the folder was inspected. These source changes are not
in alpha.5 and do not establish deployment or recovery on your machine.

## Unexpected disk activity while recording changes

First distinguish the session folder from the Git repository Mike resolves for it.
In older source `0ad4acd5`, the change-record watcher resolves session folders with
`git rev-parse --show-toplevel`; a folder without its own repository can therefore
resolve to an enclosing repository. Recording file snapshots uses `git hash-object -w`,
which writes objects to that repository. This is separate from database size and from
build-cache size.

On the machine owning the session, record its working folder, installed build identity,
effective settings and the time of the activity. These commands read metadata and disk
usage without changing the repository; substitute the actual session folder:

```sh
git -C /path/to/session-folder rev-parse --show-toplevel
git -C /path/to/session-folder rev-parse --path-format=absolute --git-common-dir
git -C /path/to/session-folder count-objects -vH
df -h /path/to/session-folder
```

Compare the resolved root with the project you intended to watch. If necessary, run
`du -sh /actual/git-common-directory/objects` on the directory returned above, not an
assumed `.git` directory (a worktree's `.git` may be a file). Disk-usage enumeration
can itself be expensive on a large object store: take a bounded diagnostic snapshot,
not a tight polling loop. These readings alone do not establish which process caused
the growth. Keep paths and logs private or redact them before sharing.

Check the **effective** `change_record` value in Settings, not just its compiled
default. In the post-alpha.5 source described in [Settings](settings.md#choose-how-code-findings-reach-a-session),
the `recommended` preset enables recording even though the compiled default is off.
An installed build and its stored overrides can differ from the current source.

If the operator chooses to pause `change_record`, it requests the watcher to stop at
the supervisor's next settings check; an in-progress scan may finish first. The tradeoff is a gap in
new change history and dependent fleet evidence and code scans. It is not a disk
cleanup: existing objects and database records remain. Nor does it cancel queued
feedback. `code_scan` controls scanning, `code_findings` controls findings delivery,
and `code_findings_push` controls proactive delivery; turning off a feedback switch
is not the same as stopping the file watcher.

### Newer watcher behavior

[Issue #428](https://github.com/temet-ai/mike/issues/428) is now closed and
[PR #430](https://github.com/temet-ai/mike/pull/430) merged. Source `e81b613c` includes:

- An enclosing repository is watched only when the session folder is its top-level
  directory or contains files it tracks. This is not a declared-checkouts-only policy:
  the resolved parent repository can still be watched when that condition holds.
- Build directories such as `target`, `node_modules`, `dist`, `.target*`, `dist-out`,
  `__pycache__` and `.venv` are excluded from watcher discovery/events and Git scans,
  even if they are not gitignored.
- During full scans, size/mtime caching avoids rehashing unchanged files; notification
  paths still hash directly. Recently modified files are
  not newly cached until their timestamp is at least two seconds old. Ignored-path
  caching is cleared by a `.gitignore` event, a full check or exceeding its 10,000-entry
  bound. These are performance mechanisms, not an assurance that no files are read.
- For ordinary files, the watcher chooses whether to write Git snapshot objects using
  a **1 MiB metadata-size threshold** checked before hashing. Larger files are still
  hashed without writing new objects. Their content
  may therefore be unavailable to later blob-based authorship or code-scan work.
  This is not a strict content-size/security boundary, a total disk quota or a
  hashing-size limit.
- With notifications working, the default full-check interval is ten minutes; without
  them, fallback polling is thirty seconds. Startup still establishes a baseline, and
  overflow-triggered rescans are throttled to at most once per minute. Periodic scans
  remain; this is not a notifications-only watcher.

A later post-alpha.5 change, [PR #432](https://github.com/temet-ai/mike/pull/432),
merged at `a89393e4`, adds two narrower guards:

- The supervisor's default `skip_temporary` setting skips session folders under
  `TMPDIR`, paths beginning `/tmp/`, paths containing `/.claude/.cache/tmp/`, and
  a `scratchpad` path component. This is path-based filtering, not a guarantee that
  every disposable repository is recognized, and not a user-facing Settings toggle.
- On the notification-path observation route, a burst with an observed deletion checks
  the checkout's `.git` entry. If it is absent, or disappears after a two-second wait,
  the burst is not recorded and that watcher stops. A `.git` file counts as present,
  as it should for a worktree. This guard does not cover every teardown timing or the
  full-scan/overflow route, and does not prove that every deletion is a teardown.

When expected history is missing, first verify the installed build, effective recording
setting, session folder and resolved Git root. On the owning machine, use read-only
`git -C /actual/checkout status --short`, `git -C /actual/checkout diff --stat` and
`git -C /actual/checkout show --stat <commit>` to compare the current checkout and the
specific commit with the record. Check `git worktree list --porcelain` from a surviving
checkout if the original folder has gone. These are different evidence sources: neither
an absent record nor a clean current diff proves that no earlier edit happened. Redact
paths and commit details before sharing.

Compare the installed build with that source before expecting these protections.
Merged source is not evidence of deployment, a released binary or measured improvement
on your machine. Older objects and database records are not removed by these changes.
Do not delete `.git`, prune objects, clear change records or remove worktrees as a
generic remedy: recorded snapshots may depend on those objects. Preserve evidence and
establish the actual scope and retention requirements before choosing any cleanup.

## Report a problem

Open an issue at <https://github.com/temet-ai/mike/issues> and include:

- the output of `mike --version` and `mike-mcp --version`;
- your operating system and architecture, and how you installed (Homebrew, mise or the archive);
- what you ran and what you expected;
- the output of `mike doctor`, and the last lines of the log.

Read everything first. Logs and `mike doctor --support-bundle` hide credentials, but they can name your folders and projects. Never attach a database dump, a backup, your configuration file or captured conversation text.
