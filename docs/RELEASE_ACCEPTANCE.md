---
title: Local archive acceptance helpers
status: active
created: 2026-10-13
updated: 2026-11-06
summary: The six alpha-gating cases and how each is recorded, the opt-in installed archive journeys, exact evidence boundaries, and the work left after the alpha.
---

# Local archive acceptance helpers

`scripts/release-acceptance` consumes a trusted candidate directory containing
the manifest and both archive/checksum pairs. It verifies and installs their exact
bytes into an owned temporary directory outside the checkout. It never builds,
downloads, publishes, or silently replaces an archive with a development binary.

```
scripts/release-acceptance --candidate /private/candidate --output /private/new-evidence
```

Output must be a new directory outside the checkout, candidate and any selected
predecessor directory. The report and summary are private. Exit 1 means a helper
failed; exit 2 means a gating case remains NOT RUN; exit 0 means every case that
gates an alpha holds a PASS.

**Alpha scope (operator ruling).** The alpha bar is gg's: install and run, plus every
case that can lose data. These six cases gate an alpha (`gates_alpha: true`,
`required: true`); every other case keeps its NOT_RUN row with `gates_alpha: false`
and does not gate an alpha:

| Case | Recorded from (only when every check of that journey PASSED on this candidate's manifest) |
|---|---|
| AC08 | `disposable_local_install`, `installed_cli_smoke`, `repeat_install_and_cli_smoke` (fresh HOME, empty PATH; not a new OS account) |
| AC01 | the three install checks, `--trial-journey` and `--persistent-journey` (search finds the capture before and after a durable restart) |
| AC02, AC03, AC07 | `--persistent-journey` (migrations, streaming capture, restart) |
| AC22 | `--persistent-journey --maintenance-journey` (backup, verify, fresh restore) |

A PASS row carries `observed.source` and the passed check ids; `command` and
`exit_status` stay null because the journeys run in-process. A failed, unbound or
unrun journey moves no case. Without the journeys that need the PG19 fixture, the run still exits 2.
AC23 and AC24 (upgrade and reinstall) are out of the alpha: they stay in the ledger,
do not gate, and are NOT_RUN unless `--upgrade-predecessor` is given a trusted frozen
predecessor and that journey passes (source `archive_upgrade_journey`). No alpha
candidate has such a predecessor.

Beyond the alpha cases above, the notes below that say a journey "remains NOT_RUN"
describe a restricted runner, not a fact about the project; a runner with Docker, mise and
the PG19 test database can run them.
The evidence ledger retains all 33 canonical IDs: AC01–AC27, SG01–SG03,
PUBLIC01, PUBLIC02 and FEEDBACK01. Each case includes its requirement (`required_before`),
exact candidate binding when verification reaches the manifest, expected/observed
result, command/exit/timing slots, test-count slots, evidence references and
structured blockers. Missing observations stay null, not invented zero exits or
passing counts. Helper elapsed times are measured separately and do not qualify
the case.

AC26 and FEEDBACK01 are post-publication work, while PUBLIC02 is conditional on
source exposure. Their visible NOT_RUN states must not be mistaken for circular
prerequisites to preparing a first reviewed alpha. Source gates, complete
platform assertions and independent content/rights review remain separate from
the helper regression tests.

The optional drivers documented below are implemented helper code.
Their installed mode/PTY, two-version upgrade/reinstall, native browser boundary,
fleet onboarding and budgeted resource observations remain NOT_RUN on the
restricted implementation host. Socket-free mock, parser and filesystem contract
tests are regression evidence, not installed-candidate or release qualification.
The `normal_defaults` reliability profile has a separate isolated-runner adapter,
endpoint/default contracts and synthetic workspace/harvester worker assertions.
Only offline contracts have run; actual isolated candidate execution remains
NOT_RUN. Direct host execution of that profile fails closed, as detailed below.

## Explicit optional journeys

`--trial-journey` additionally exercises an owned foreground trial process,
synchronized synthetic SSE, inspect metadata, packaged HTTP assets/deep links,
raw access-boundary reads/upgrades, the installed MCP stdio companion, and safe doctor/support JSON. Trial cannot
establish durable capture, and HTTP assets cannot establish browser execution.

The raw boundary helpers run against both installed inspect and SPA listeners after
the synthetic request is captured. On each listener, both the stored-request-body
GET route and WebSocket upgrade route receive these 18 rejection probes:

- Forged host, confusable host, wrong listener port, duplicate Host
- Disallowed, null, wrong-scheme, malformed and duplicate Origin
- Originless cross-site, same-site, invalid and duplicate Fetch Metadata
- Forwarded headers attempting to authorize a disallowed origin
- Missing Host in HTTP/1.1 and HTTP/1.0, absolute-form HTTP/1.0 without Host,
  and conflicting absolute-form request authority

Requests are emitted as raw bytes to `127.0.0.1`, retaining missing/duplicate
headers and HTTP versions without a high-level client's normalization. Rejections
must be 400/403, contain none of the planted content/session markers in body or
headers, and grant neither WebSocket upgrade nor CORS access. A policy 403 must
contain only an error envelope. Native, same-origin, and approved originless Fetch
Metadata requests must still read the exact captured body or complete a valid
RFC 6455 handshake, with another legitimate check after all rejection probes.

The wire client has per-request absolute deadlines and header/body/wire caps,
rejects ambiguous framing, closes every owned socket, and follows no redirects.
WebSocket observations end at the handshake; event delivery and browser execution
remain NOT_RUN. These read/handshake helpers do not qualify mutation protection,
all browser behaviors, or the complete AC06 case.

`--persistent-journey` additionally exercises candidate-owned migration of a fresh
database, synthetic capture, and foreground stop/restart from a second unrelated
working directory. It requires an already running, explicitly owned disposable
PostgreSQL 19 fixture selected with both:

- `MIKE_TEST_ADMIN_URL`: password-free `postgres` admin connection, exactly
  `postgres://postgres@127.0.0.1:PORT/postgres?sslmode=disable`. The nonprivileged
  custom port cannot be 5432 or 5440.
- `MIKE_RECOVERY_TEST_PGDATA`: the connected server's owner-private, non-symlink
  PostgreSQL 19 data directory. The server must listen only on `127.0.0.1`, have
  no Unix socket directory, and report this exact data directory.

The verifier needs PostgreSQL 19 `psql`, `pg_dump`, and `pg_restore`, as required by
the shared disposable recovery-fixture validator. Explicit optional
`MIKE_RECOVERY_TEST_TOOL_DIR` and `MIKE_RECOVERY_TEST_LIBRARY_PATH` can select
relocated verifier clients/libraries. They are never inherited by Mike; candidate
runtime PATH is empty. Required server extension packages are documented in
[Installed database provider](INSTALLED_DATABASE.md#existing-postgresql).

```
MIKE_TEST_ADMIN_URL='postgres://postgres@127.0.0.1:6432/postgres?sslmode=disable' \
MIKE_RECOVERY_TEST_PGDATA=/private/disposable-pg19 \
scripts/release-acceptance --candidate /private/candidate --output /private/new-evidence \
  --persistent-journey
```

Missing, malformed, foreign, or unavailable fixture inputs fail; they are never
reported as skipped or passed. The helper does not start, stop, or modify cluster
configuration. It creates a uniquely named empty database, checks that it is empty,
and drops only databases allocated by this run after owned process cleanup.
Failure to clean up is reported as a failure rather than concealed.

Only the exact installed `mike serve` applies migrations. The helper never seeds
application tables, uses recovery-test synthetic migration rows, applies source
SQL files, or modifies checksums to make a candidate start. On both generations,
strict doctor reports must identify the selected clean build and DATABASE_CURRENT.
A separate read-only full-ledger check requires all successful rows and records a
hash over versions, descriptions, timestamps, checksums, success and execution
times. There is no hardcoded migration count.

The first captured SSE event must reach the client before the fixture releases its
remaining events. Both generations must expose the same single request/session
identity, exact stored request and response bytes/hashes, healthy reachable persistence,
unchanged full ledger, and matching installed MCP reads. Each launch rechecks the
manifest and installed executable hashes. The config and planted unrelated `.env`
files must remain unchanged. Shutdown signals only the retained owned foreground
process group, requires a zero exit status within a deadline, and does not adopt
pidfiles or other listeners.

Streaming response storage is assembled message JSON. The helper verifies its
fixture text, tool input, model and usage, then compares that stored representation
byte-for-byte across restart. Raw SSE transport bytes are checked separately and
are not confused with the persisted response representation.

This foreground SIGTERM/serve helper does not qualify the installed CLI/TUI
start/stop flow, browser rendering, Docker provisioning, real Claude,
logout/autostart or native clean-account installation. It is the source of the
AC02, AC03 and AC07 rows (and of AC01 and AC22 with the journeys named in the alpha
table), and of no other case. Offline tests of its mock orchestration are helper regression evidence only.
If the execution environment forbids required process/socket operations, keep
those observations NOT RUN; do not bypass the restriction or fabricate evidence.

## Optional selected-installation mode and PTY journeys

`--mode-journey` exercises the installed `setup` CLI against a separate fresh
fixture using the explicit disposable PostgreSQL 19 inputs above. It does not
require `--persistent-journey`. Add `--mode-tui-journey` to repeat the sequence
through the installed setup/control TUI in an owned bounded PTY; this second flag
requires `--mode-journey`.

```sh
scripts/release-acceptance --candidate /private/candidate --output /private/new-mode-evidence \
  --mode-journey --mode-tui-journey
```

Each surface gets its own private config/HOME and empty database. The sequence is
trial → PostgreSQL → trial → PostgreSQL → restart from an unrelated working
directory, with an additional database restart after supported feature isolation.
It compares the requested, saved and live modes, distinct boot generations,
installed hashes and strict doctor reports, including stopped trial/database
states without starting a daemon. Trial capture must not be imported into the
database, and database history must not appear as trial history. Returning to
PostgreSQL and restarting must preserve the same durable request/session, exact
body hashes and full migration ledger without duplicate capture.

The CLI supplies a literal database URL from a private file; the PTY types it
through the masked URL prompt. Both include synthetic shell-special characters.
Config backups must preserve prior bytes privately, and planted unrelated `.env`
files and URL inputs must remain unchanged. Refusal subchecks cover missing trial
loss acknowledgement, conflicting persistence/fallback/URL environment overrides
and malformed literal URLs before config changes or daemon launch. Raw terminal
output and typed URLs are not retained; reports contain hashes and named stages.

Installed setup detaches its daemon. Cleanup therefore requires Linux
`SO_PEERCRED` and permitted private AF_UNIX control, verifies the selected lease,
identity, peer and exact generation, then requests cooperative stop. It never
signals a PID read from disk. Unresolved cooperative cleanup fails and retains
private control state for inspection rather than deleting it or adopting another
process. The CLI restart subcheck uses this private stop followed by installed
setup because there is no standalone CLI stop command; the PTY subcheck uses the
installed TUI stop/start keys.

These are implemented automated prompt/key and mode-continuity subchecks, not
visual/manual native-account TUI evidence. Managed Docker transitions, real client
registration, logout/autostart and degraded/outage/capture-gap doctor states
remain NOT_RUN. The fixture uses synthetic isolation overrides, not normal-default
reliability. Denied sockets or PTYs must not be worked around. Every full case,
including AC13–AC16, remains NOT_RUN.

## Optional installed browser subchecks

On a verifier host that permits Chromium, opt in with existing trusted executables
and the explicitly owned PostgreSQL 19 fixture described above:

```sh
scripts/release-acceptance --candidate /private/candidate --output /private/new-evidence \
  --persistent-journey --browser-journey \
  --browser-node /absolute/node --browser-chromium /absolute/chromium
```

The verifier needs the repository's existing Playwright dependency. It installs or
fetches nothing, does not run Vite, attach to a browser profile/debugging port, or
disable Chromium's sandbox. Mike's runtime PATH stays empty. A restricted host
must record this execution NOT RUN rather than try a sandbox workaround.

The browser helper runs after the baseline persistent journey and any requested
maintenance roundtrip complete, with a separate fresh unseeded database, private
config/HOME, session, ports and owned foreground server. Only the verified
candidate applies migrations. Source identity, migration ledger, stored bodies and
the persistence-backed session APIs must pass before browser use. Additional
synthetic browser captures never enter the restart or maintenance fixture.
Trial mode lacks the session and response-body APIs needed by these DOM checks;
`--trial-journey` alone does not enable the browser helper.

The packaged UI driver visits only its owned fixture's exact loopback origins. It
checks hydrated request/tool/response DOM, direct links and reloads, actual selected
endpoints, packaged help navigation, native WebSocket frames and a second
synthetic capture's live count. External requests and redirects are refused.
The only allowed browser POST is the exact local SPA delta-fallback diagnostic
with a bounded JSON body and allowlisted reason. Relevant console/page/script
failures still fail the helper; no error response is mocked or suppressed.

The fleet onboarding driver checks an empty-project screen with the selected
proxy command, a single project's automatic selection, deterministic selection
from multiple projects, and explicit project URL reload/back navigation. It
assigns only the synthetic session through the public project-assignment API;
synthetic project paths are database values, not operator filesystem paths to
create or inspect. This is a bounded subset of onboarding states.

The same `--browser-journey` also runs a separate native-browser boundary driver
against both inspect and SPA listeners. An owned hostile-origin page and a
sandboxed iframe without `allow-same-origin` exercise genuine browser-generated
hostile and `null` origins. Per address family, it requires eight rejected reads
(CORS and no-CORS), four rejected mutation preflights and four rejected WebSocket
connections. Native network observations must show server 403s, the expected
Origin/Fetch Metadata and no mutation PATCH after a rejected preflight. It checks
unchanged stored body/session state, successful same-origin reads and reversible
pin mutations, and actual text/tool/completion events for one new synthetic
capture on legitimate sockets while rejected sockets receive no events.

Probe fetches, CORS responses and WebSocket handshakes stay native; the helper does
not replace rejection responses or synthesize Origin headers. Only owned document
and favicon navigation uses redirect-refusing response passthrough. Request guards
and a fresh browser context constrain the test but are not an OS network sandbox.
Only allowlisted observations survive in reports; browser logs and raw fixture
content are not retained.

Add `--browser-ipv6` to opt into the implemented IPv6 boundary driver. It requires
`--browser-journey`, stops the IPv4 candidate first, and launches a separately
configured owned foreground candidate on `::1` using V6ONLY port reservations.
Only this browser journey's database is reused; the ledger and private config
files must remain unchanged. Omission records the IPv6 subcheck NOT_RUN; an
explicitly requested but unavailable IPv6 fixture fails without address fallback,
wildcard binding, OS network changes or borrowed listeners. IPv4 success never
counts as IPv6 evidence.

These implemented DOM/onboarding/security subsets do not establish all browser
states, browser continuity across restart or complete AC04/AC06/AC18 evidence.
Installed Chromium execution, including hostile/null and IPv6 checks, remains
NOT_RUN on the restricted host. The helper leaves every full case NOT_RUN and
cleans up only its owned servers and databases, leaving the selected PostgreSQL
cluster running.

## Optional installed maintenance roundtrip

Add `--maintenance-journey` to `--persistent-journey` to exercise the exact shipped
maintenance kit after both owned source generations have stopped. The same
explicit disposable PostgreSQL 19 fixture and maintenance-tool prerequisites
apply. No source database or host cluster is adopted or reconfigured.

The helper uses an explicitly synthetic private data file, backs up the stopped
fixture, verifies the bundle, allocates a new empty owned target, and restores
through the installed kit. It compares restored config/data/state bytes and the
full ledger before rebinding only the restored fixture's database URL/data root
and identity-derived state location. It starts the same hash-bound binaries,
checks stored capture/ledger/MCP continuity, then stops its owned process. The
source filesystem and ledger must remain unchanged. All allocated target
resources remain under the original fixture cleanup even if a later step fails.

The maintenance implementation also accepts the exact shared-root source layout
`config=data/config.toml`, `state=data/state`, preserving existing macOS defaults.
The unchanged version-1 bundle assigns each file to one logical config/data/state
scope and prunes nested state from data traversal, including lifecycle exclusions.
Portable filesystem fixtures exercise backup/verify/fresh restore, unchanged source
bytes, private outputs, unsupported overlap and symlink/traversal/race refusal.
They use a fake database and do not qualify native macOS or the installed journey.
Existing disjoint-root bundles and fresh offline-review restore semantics remain
unchanged; no live files or installation identity are relocated.

Short owned paths are required for portable control-socket limits. An existing
backup, foreign target, changed kit, failed source stop, mismatched restored data,
or failed cleanup is an error. With a passing persistent journey this roundtrip
is what records AC22. It does not qualify real-user backup, published-predecessor
upgrades, interrupted installation, retained-state removal, or AC23 and AC24.
Mock tests validate the harness contracts only; a PASS for AC22 exists only in a
ledger written by a real run on a candidate.

## Optional two-version upgrade and retained reinstall

`--upgrade-predecessor /private/trusted-predecessor` opts into the implemented
populated two-version helper independently of `--persistent-journey`. It requires
the same explicitly owned PostgreSQL 19 fixture and verifier clients. Supply a
real, explicitly trusted, frozen predecessor directory containing its manifest and
both archive/checksum pairs. The helper never chooses, downloads, rebuilds or
fabricates a predecessor to obtain a passing result.

```sh
scripts/release-acceptance --candidate /private/candidate --output /private/new-upgrade-evidence \
  --upgrade-predecessor /private/trusted-predecessor
```

Both archive pairs must target the same native platform, bind distinct source
commits, and have strictly increasing SemVer precedence. A changed same version
or build-metadata-only change is not an upgrade. The candidate's real migration
inventory must strictly extend the predecessor's unchanged migration prefix;
same-schema pairs cannot establish this incompatible-predecessor test. Both
versions need complete hash-bound installed maintenance kits and compatible
capture/feature APIs. Missing trusted artifacts or a suitable genuine migration
extension means installed evidence remains NOT_RUN, not permission to rename a
version, seed a fake ledger or edit migration checksums. Supplying an invalid pair
fails the helper; no full case is promoted or reported as skipped/passed.

Only the exact archived executables migrate a newly allocated empty database. The
helper captures synthetic durable data with the predecessor, stops its writers,
and creates and verifies an unchanged-source backup using that installed kit.
It injects SIGKILL into its owned installer immediately before and immediately
after the atomic paired-generation selection swap. The selected pair must be
respectively the predecessor or candidate, durable state must be unchanged, and
retrying exact installation must be idempotent. This is installer-boundary
interruption, not interruption during a running database migration.

The candidate then applies only its extension, preserves every old ledger row and
capture identity/body hash, and adds a second durable capture. Installer downgrade
refusal must leave the installation tree unchanged. Direct startup of the retained
predecessor against the genuinely newer schema must exit nonzero for a recognized
migration incompatibility, leaving the full logical database dump and durable
config/data/state unchanged. This comparison excludes lifecycle files and
diagnostic logs; it is not a claim that no ephemeral file was touched.

While the owned candidate is running, installed MCP register/unregister commands
operate only on fresh synthetic client JSON. They must preserve unrelated entries,
keep private byte-exact backups, probe the owned instance and refuse changed or
foreign registrations. After stopping writers, the installed maintenance kit
unlinks only its owned command pair, preserves database/config/data/state/backups,
recovery generations and an unrelated command sentinel, and reinstalls the exact
same candidate. A further owned foreground start verifies capture, ledger and MCP
continuity.

These helpers are implemented but their installed two-version execution remains
NOT_RUN on the restricted host. Mock contracts do not establish a real predecessor
upgrade, migration interruption, purge, native removal, real Claude integration,
service/autostart behavior or published-channel installation. The upgrade and
reinstall journeys are out of the alpha, and the first alpha has no published
predecessor to upgrade from. AC23 and AC24 record PASS only from this journey
on a real trusted predecessor pair; every other full case outside the six alpha
cases stays NOT_RUN.

## Optional installed reliability workload

`--reliability-journey` opts into a separate fresh runtime/database after any other
selected journeys. It requires the same already running, explicitly owned disposable
PostgreSQL 19 fixture described above. Neither `--persistent-journey` nor a browser is
needed for this separate helper. There is one policy and no stages. For example, on
an unrestricted verifier host with the trusted candidate:

```sh
MIKE_TEST_ADMIN_URL='postgres://postgres@127.0.0.1:6432/postgres?sslmode=disable' \
MIKE_RECOVERY_TEST_PGDATA=/private/disposable-pg19 \
scripts/release-acceptance --candidate /private/candidate --output /private/new-reliability-evidence \
  --reliability-journey --reliability-duration-seconds 3600 \
  --reliability-concurrent-sessions 8
```

The default is at least 900 real monotonic seconds and one synthetic session.
`--reliability-duration-seconds` must lie between 900 and 7200 seconds, and
`--reliability-concurrent-sessions` between 1 and 32. Duration/concurrency options
without `--reliability-journey`, zero values, shortened runs, and nonfinite durations
are rejected before creating evidence. There is no short-run CLI override.

Each batch proves that the requested distinct sessions overlap at the owned
synthetic upstream barrier, then verifies exact streamed bytes and durable
request/response bodies. A ten-second delay follows completed batches, shortened
only at the final duration boundary. Sessions are idle between batches; this is
not continuous saturation. Client work, fixture handlers, request counts, body
sizes, API responses, deadlines and cleanup are bounded. No request is retried or
resent. Read-only final database accounting rejects missing, duplicate, foreign,
incomplete or replaced captures and missing or deduplicated durable tool records.
Synthetic message/tool wire IDs are unique per
request to respect global tool-record deduplication. Reports retain counts,
bounds, hashes and static error codes, rather than bodies, secrets or raw logs.

The report profile is `synthetic_isolation_overrides`, with safe override names
listed explicitly. The initial setup process may start background workers against
its empty owned database because database/compiled feature defaults override TOML.
Before any measured traffic, the helper disables every reviewed toggle through
the supported feature API, verifies the live flags, gracefully stops and restarts
its owned process, then re-verifies every flag. Missing, unknown or still-enabled
flags fail closed. Live flags are checked again throughout the measured workload.
Embedding must report `reachable_state=not_configured`, `reachable=false`, and
its compiled URL `http://127.0.0.1:7997`; the provider key must be absent. The boot
loader resets embedding configuration after TOML/environment loading, so this
helper emits no ineffective embedding override. That disabled endpoint is not
fixture-owned and must remain unused. Supported upstream, Finite and OpenRouter
URL overrides point to the owned synthetic fixture. These are service
prerequisites, not proof of all-transport network isolation or descendant
containment. This modified profile does not establish AC20's normal-default
behavior. Explicit selection is also available through
`--reliability-profile synthetic_isolation_overrides`.

### Explicit resource, drain and responsiveness budgets

Add `--reliability-resource-budgets /private/reviewed-budgets.json` with a
`--reliability-journey` to opt into the implemented observations. The regular,
non-symlink JSON file is bounded to 16 KiB and must contain exactly these six
unique keys, with no extra keys or implicit acceptance thresholds:

- `rss_peak_bytes`: positive integer ceiling on the owned daemon's lifetime RSS
  high-water value
- `fd_peak`: positive integer ceiling on each observed daemon descriptor count
- `broadcast_backlog_peak`: nonnegative integer ceiling on the sampled
  `proxy_memory_broadcast_backlog` metric
- `active_streams_peak`: positive integer ceiling on `/api/stats` active streams,
  at least the requested concurrent-session count
- `drain_seconds`: finite positive deadline, at most 30 seconds
- `responsiveness_seconds`: finite positive deadline, at most 30 seconds

The four integer budgets cannot exceed 2^53; booleans, NaN, duplicate keys,
missing fields and invalid types/ranges are rejected. These are schema bounds,
not recommended product targets. The operator must supply reviewed budgets;
example mock-test numbers are not acceptance policy. Without a budget file,
resource/drain/responsiveness observations stay NOT_RUN and only harness bounds
are reported.

Linux procfs observations bind the exact daemon PID/start time to its retained
owned supervisor. They exclude PostgreSQL, the supervisor, MCP and Chromium.
Sampling occurs every 0.25 seconds plus explicit checkpoints; reports distinguish
sampled RSS from the kernel process-lifetime RSS high-water value and retain
baseline/final deltas. FD peak is only a sampled lower bound, not a kernel
high-water measurement or proof that no between-sample spike occurred. Missing
procfs data or changed process identity fails, rather than recording zero usage.

Product observations check active streams while the synthetic upstream batch
barrier is held. Only the named broadcast backlog is sampled; drain requires zero
active streams and backlog plus a subsequent successful memory-ticker sample
within the explicit deadline. Other internal queues remain NOT_RUN. Responsiveness
checks use the installed MCP companion's `diagnostic_memory` read and the embedded
SPA HTTP endpoint, with live metrics/stats response timing also bounded. SPA HTTP
latency is not rendered-browser interaction latency. The baseline is pretraffic
idle versus workload in the same owned process, not another build, normal-default
control or real-provider performance.

### Normal defaults: isolated adapter implemented, runtime unqualified

`--reliability-profile normal_defaults` requires `--reliability-journey`, explicit resource
budgets and a standalone invocation. It cannot be mixed with the trial,
persistent, browser, maintenance, fault, installed-mode or upgrade journeys.
The route copies and hashes bounded candidate input bytes on the host, but never
executes the host archive/install smoke or candidate. Archive verification,
installation and all candidate commands belong inside the isolated runner.

Execution additionally requires all four explicit options:

- `--normal-runner-image`: an already-local trusted repository image pinned by
  its immutable `@sha256:` digest. The adapter never pulls an image.
- `--normal-runner-docker`: the absolute path of the trusted Docker executable.
- `--normal-runner-endpoint`: the absolute path of the existing local Unix socket.
- `--normal-runner-runtime`: an already-configured custom native runc alias whose
  daemon-reported executable path is absolute. Reserved/default relative runtime
  paths are unsupported; the helper never installs or configures a runtime.

Without these options the route fails closed before even copying inputs. Initial
support is native Linux x86_64 with an existing rootful cgroup-v2 Docker engine.
Docker Desktop, remote daemons, rootless engines, macOS and unsupported or
unreadable enforcement observations are rejected. This is an explicit opt-in
runner, not permission to install Docker or alter host security/network settings.

The adapter creates only a fresh, ownership-labelled container with no network,
no host mounts or Docker socket, a read-only root filesystem, nonroot identity,
all capabilities dropped, no-new-privileges and bounded CPU, memory, process and
private executable tmpfs resources. The initial process is a trusted bootstrap.
Before transferring any executable candidate payload, the host verifies the
exact daemon configuration and independently observes the live process,
namespaces, mount flags and cgroup limits. A pinned, explicit default-deny
seccomp profile permits only native x86-64 syscalls and AF_UNIX/INET/INET6 socket
domains; compat socketcall, io_uring and namespace/admin escape paths are denied.
The exact profile JSON must match daemon inspection, and the daemon-selected
runc executable must have an explicit absolute path with a verified, hash-bound
native ELF64 x86-64 identity. Relative/default runtime paths are unsupported.
Unsupported syscalls fail safely without a broader fallback. Actual profile compatibility and
security-boundary execution remain NOT_RUN until authorized runtime validation.
Inner candidate output cannot attest to that outer isolation. Cleanup rechecks exact container ownership; uncertain
cleanup is reported with the retained container identity, not broadened to other
containers. Runtime use requires an independently prepared trusted image meeting
the [runner image contract](RELIABILITY_RUNNER_IMAGE.md) for Python/PostgreSQL
19 and extensions.

Inside that boundary the workload uses one fresh database, private HOME and one
normal-default boot. It relocates service endpoints while preserving every other
compiled setting. It compares the complete effective boot configuration, all 218
runtime settings and all feature defaults, then checks they remain unchanged
through the workload. There is no secondary database or real provider credential.
Embedding stays at its compiled disabled default and URL `127.0.0.1:7997`; the
loader ignores embedding TOML/environment overrides, so this is not an owned
fixture endpoint. Bounded Finite list/manifest responses follow the source wire
models, while the default worker workload expects no Finite or embedding calls.

The dedicated fixture creates a synthetic project/location and a trusted
system-prompt working-directory marker, then emits valid unique `<mem:update>`
tags. API and read-only owned-database assertions check workspace registration,
harvested lessons, provenance and their post-stop durability binding. These
triggers are separate from the older modified-profile `Cohort`. The report keeps
operator-driven fleet/assistant work, internal worker cycles and embedding
consolidation explicitly unexercised. Matching source defaults alone does not
prove every background worker ran.

Only socket-free mocks, parser and filesystem contracts have been executed on
the implementation host. No container, candidate, database, real timed workload
or native runtime was executed for this adapter. Its code is implemented but
runtime evidence remains NOT_RUN. Stub latency is not real-provider performance;
SPA HTTP checks are not rendered-browser responsiveness. Every full acceptance
case, including AC20/AC21, stays NOT_RUN. A successful partial journey still exits
2 (incomplete); a failed prerequisite/helper exits 1. Fault injection remains a
separate workload and does not establish normal-default reliability.

## Optional installed upstream fault checks

`--fault-journey` opts into a separate fresh database, config, synthetic sessions
and foreground candidate process. It requires the same already-running, explicitly
owned disposable PostgreSQL 19 fixture described above, but does not require
`--persistent-journey`, a browser, or the reliability workload. When combined with
other helpers, it starts only after their owned runtimes have been cleaned up.
For an unrestricted verifier with trusted archives:

```sh
MIKE_TEST_ADMIN_URL='postgres://postgres@127.0.0.1:6432/postgres?sslmode=disable' \
MIKE_RECOVERY_TEST_PGDATA=/private/disposable-pg19 \
scripts/release-acceptance --candidate /private/candidate --output /private/new-fault-evidence \
  --fault-journey
```

This policy-independent subset sends seven uniquely identified synthetic requests:
a baseline, HTTP 429, recovery, HTTP 500, recovery, a cancelled stalled SSE stream,
and recovery. Buffered 429/500 exchanges must preserve status, exact body and
provider/rate-limit/Retry-After headers. They are captured as `Complete` HTTP
exchanges with the corresponding error `response_status`, not as successful model
answers. The fixture observes exactly one upstream attempt per request; the
product's connection-error retry path is not exercised here.

The cancelled stream sends a partial text delta and then stays silent. The
client closes its owned connection after observing that text; the fixture must
observe the proxy closing its upstream connection before the deadline, without
releasing more data or finishing the stream to provoke cancellation. With
`max_in_flight=1`, the following capture must succeed. Durable checks require an
error-terminal request, its abort record and exact partial text block. They do
not claim that cancellation writes a complete assembled response or retains the
entire original request body. A further owned restart must preserve the durable
capture identities, completed response bodies and partial/error state.

As with the reliability helper, this uses `synthetic_isolation_overrides`, not
normal defaults. Initial setup workers may start against the empty database;
all 32 reviewed feature toggles are disabled through the supported API, and an
owned restart plus live flag checks precede capture traffic. Missing or enabled
flags fail closed. Shared readiness also checks disabled embedding at its compiled
URL and an absent provider key, without claiming ownership of that unused endpoint
or all-transport network isolation. Candidate hashes/source identity, migration inventory and
ledger are checked; only owned process groups, sockets and newly allocated
fixture databases are cleaned up. Client/server deadlines, concurrent handlers,
body sizes and cleanup are bounded. Reports retain identities, hashes, bounds
and static error codes, not raw payloads or logs.

Database outages, quota-limited disk exhaustion, persistence-failure forwarding
policy, interrupted-write recovery and any supported loss window remain NOT_RUN.
They require an explicit documented policy and separate evidence; these checks
must not silently adopt current data loss as an acceptable contract. No real
provider, live database or host disk is faulted. Every full acceptance case,
including AC21, stays NOT_RUN. Partial success still exits 2 (incomplete), and
helper failure exits 1. Only socket-free mock contracts have run on the restricted
implementation host; the installed fault journey itself has not run there.
