---
title: Release acceptance matrix
status: active
created: 2026-11-05
updated: 2026-11-06
summary: The 33 stable case IDs of the release acceptance ledger, with the observation that makes each one pass; the alpha bar is the six cases named in RELEASE_ACCEPTANCE.md.
---

# Release acceptance matrix

This is the policy the acceptance ledger and the publication qualification point at
(`POLICY` in `scripts/release/acceptance_ledger.py` and `publication_gates.py`). It lists
what has to be observed; [`RELEASE_ACCEPTANCE.md`](RELEASE_ACCEPTANCE.md) says which cases gate
an alpha and how the local helpers record them. The cases are required observations, not
completed tests. The verifier records PASS, FAIL or NOT RUN for every case. A case changes state
only from a real journey's observation on the candidate, never from helper smoke, a fixture or
a mock. A required NOT RUN blocks qualification. A feature or platform left out of the release
is marked NOT SUPPORTED in the release contract, not reported as tested.

**The alpha bar** is install and run, plus every case that can lose data: AC01, AC02, AC03,
AC07, AC08 and AC22. Upgrade and reinstall (AC23, AC24) are out of scope for the alpha. Every other case keeps its row and does not gate an alpha.
There are no milestone stages and no invited pilot: the candidate is built from `HEAD`, installed
from its archive on a clean account, and the cases are recorded in the ledger.

All 33 case IDs are stable: SG01–SG03, AC01–AC27, PUBLIC01, PUBLIC02 and FEEDBACK01.

Every run is tied to the candidate source SHA, version, exact archive hashes,
OS/architecture/libc or macOS version, database image digest/version, Claude client
version where relevant, commands, exit codes and case-level observations.
Never count a debug-build probe, simulated CLI invocation, Vite preview or
smoke-mode run as an extracted release-archive result.

## Source gates

| ID | Preconditions and action | Observable pass criteria |
| --- | --- | --- |
| SG01 | Fresh checkout; isolated HOME/config; no DB env or operator services. Run formatting, structural lint, DB-free Rust tier and script regressions with declared tools. | Intended fast tier passes without DB access; no maintainer paths/rules/tool installations required; no unrelated process cleanup; real command statuses propagate. |
| SG02 | Provision explicitly owned disposable supported Postgres. Run the complete DB tier, SQLx prepare check, migration-immutability and upgrade tests. Repeat without DB fixture. | Configured run executes selected assertions and passes; missing-fixture run fails clearly before claiming success. No crawl to operator config, invisible early-success skip, shared-fixture contamination or historical checksum rewrite. |
| SG03 | Clean frozen dependency install; run strict frontend typecheck, unit/integration/Storybook suites, production build, Rust advisory/license checks and frontend advisory audit. Exercise CI routing for PR, direct main push and candidate. | Required gates pass at candidate SHA; known findings have explicit reviewed disposition, no blanket suppression; candidate runs bypass path-filter gaps. Selected/executed/ignored/excluded counts retained. |

SG01–SG03 run on the candidate's source. A docs-only release commit still runs them; a historic green run is reusable only when its exact inputs and SHA relationship are demonstrated, never assumed. A killed unfinished run cannot pass, a changed tree cannot inherit another tree's verdict, and warm caches cannot silently remove metadata.

## Short extracted-artifact gate

Run AC01–AC07 for every candidate on each target being qualified. The harness
owns a disposable DB, fake upstream and processes. The product runs outside
the checkout with a fresh HOME and no build tools on its runtime PATH. Browser
and test-driver tools may exist in the verifier environment; distinguish those
from end-user prerequisites. Normal runtime mode is mandatory, with optional
provider accounts/services absent and no `MIKE_SMOKE_RUN` shortcut.

| ID | Preconditions and actions | Observable pass criteria |
| --- | --- | --- |
| AC01 | Verify manifest/checksums; extract both binaries; run local version/help/catalogue; start from unrelated cwd. | Executables run without Rust/Node/checkout or missing shared libraries; versions/SHA agree with manifest; embedded UI assets exist; package contains only reviewed files. |
| AC02 | Fresh persistent config/data and supported disposable DB; selected loopback ports; start extracted product. | Ready means persistence enabled and reachable; embedded migrations succeed; ledger matches candidate migration manifest with zero failed rows. No hardcoded historical migration count. |
| AC03 | Controlled upstream sends valid incremental SSE including text, tool-use and final usage. Request supplies a canonical session UUID header. | First client event arrives before upstream releases later chunks; ordering and documented headers/status survive; correct session/streaming flag, content/tool/usage metadata and terminal capture state are inspectable. |
| AC04 | Browser opens packaged SPA, inspects AC03 session, navigates and refreshes its deep link. | JS/CSS and same-origin API/WS load; expected fixture text/tool result renders; deep-link refresh works; no relevant console/network failure; no Vite/source dependency. |
| AC05 | Spawn extracted MCP binary against actual selected inspect port; initialize, initialized notification, tools/list and a read tool. | Real stdio handshake succeeds; advertised profile matches; read tool returns AC03 capture from actual Mike; stdout contains only protocol output. |
| AC06 | Real browser tries allowed, unrelated and null origins; raw malformed/duplicate-origin/forged-Host checks; test both inspect and SPA ports and REST read/mutation paths. | Approved UI works; untrusted upgrade is rejected before 101 and receives no synthetic event; unrelated-origin fetch cannot read synthetic capture REST content; rejected mutation does not execute. Defined Origin-less native path works. IPv4/IPv6/custom-port cases agree with policy. |
| AC07 | Stop owned process; restart from another cwd with same config/data/DB; query API and MCP. | Same session/record IDs and exact stored request/response bodies survive; no duplication; migration ledger unchanged; health still confirms persistence. |

AC03 must use a synchronization barrier, not one buffered SSE response: the
fixture waits until the client observes the first fragment before sending the
rest. Test an explicitly configured passthrough case for wire byte identity,
then normal defaults with the product's documented mutation semantics. The
captured original inbound body remains faithful in both. Partial/cancelled
streams belong to AC21.

## Full supported-platform journey

Run against actual archives and native clean accounts/VMs. On each advertised
platform, qualify Docker and BYO paths separately. Linux container smoke tests
alone do not establish native Arch usability; a hosted macOS compile job alone
does not establish Docker runtime or Homebrew installation behavior.

| ID | Preconditions and actions | Observable pass criteria |
| --- | --- | --- |
| AC08 | New account has only documented user prerequisites. Install via local archive/download or actual Homebrew channel; repeat installation. | Both selected binaries install and run; integrity/runtime/signature policy works; PATH guidance is correct; no compilation, hidden host tool or unrelated executable overwrite. |
| AC09 | Docker/Compose present, host psql/sqlx/Rust/Node absent. Run guided Docker setup twice, then capture/restart. | Owned durable volume, correct loopback publication/credential/port, one embedded migration history; no duplicated volume or lost data; AC03/07 pass. |
| AC10 | Existing supported Postgres with extension/role variations; supply synthetic URL through protected input. | Valid target migrates and captures over two restarts. Missing version/extensions/privileges fail with specific remedy before false readiness. Each advertised DB major is tested. |
| AC11 | Configure in A; invoke TUI/serve/wrapper from B and read-only cwd; paths with spaces, custom ports, conflicting file/env/CLI values. | One intended config is selected; actual daemon obeys precedence and all overrides; UI/MCP endpoints agree; unrelated project `.env` is unchanged and cannot hijack installed boot. |
| AC12 | Enter fake password/key with spaces, quotes, dollar/backtick/backslash/hash/Unicode and a key beginning `s`; simulate interrupted writes. | Literal values round-trip without evaluation; `s` is text; secrets never appear in ordinary rendering/errors/logs/bundle; private permissions hold; interrupted write leaves valid previous config. |
| AC13 | Trial capture; configure DB while running; durable capture; restart twice; switch to trial and back. | Requested/stored/running modes agree; new persistent capture survives; old DB/history remains intact through trial. Existing trial data loss/transfer limits are explicit; no false durable-history claim. |
| AC14 | Foreign/stale/garbage/zero/negative/out-of-range pidfiles; unrelated listener; concurrent start; two configured installations. | No unrelated process is signalled; invalid records refused; one daemon per instance; foreign HTTP 200 is not adopted; two instances cannot stop each other; owned stop is graceful and repeatable. |
| AC15 | Close terminal/logout and follow documented manual restart. If login service is advertised, install it and logout/login/reboot. | Manual restart retains history and accepts new traffic. Claimed autostart starts exactly one correct instance and configuration after login/reboot. Deferred service support is explicitly NOT SUPPORTED. |
| AC16 | Missing/stopped Docker, invalid auth, missing extension, occupied ports, read-only config, startup/readiness deadline. Correct each fault and retry. | Bounded nonzero actionable failure, no false ready/no silent persistent→trial fallback; working config retained where validation failed; retry succeeds; no orphan children/foreign-resource cleanup. |
| AC17 | Existing synthetic Claude config has unrelated entries; install paths contain spaces and are absent from PATH; custom inspect port; separate Homebrew package layout. | Registration preserves unrelated data, uses stable matching executable/URL, and its actual command passes AC05. Missing binary/malformed JSON/connection failure is actionable. Upgrade does not strand a Cellar-specific registration. |
| AC18 | Empty capture DB, then one non-maintainer project, multiple projects and a project deep link; change ports. Follow first-use help and selected packaged docs with external documentation unavailable. | Empty state names working installed commands; legitimate project resolves deterministically; no maintainer-project fetch; copyable endpoint matches reality; same-origin diagnostics work. Guided TUI ready screen retains wrapped launch, standalone proxy, MCP and browser choices. Selected docs render inside the installed app, help/deep links work, and no roadmap appears; the existing shared doc list remains authoritative. |
| AC19 | Authorized real Claude Code, clean client profile and recorded version; no maintainer hooks/sidecar/herdr. Test wrapper cold/warm starts, argument/environment routing, standalone environment launch and resume. | Real request reaches provider, canonical session identity exists, text/tool capture appears in browser/MCP and persists. Wrapper arguments and selected direct-proxy environment survive shared-engine hardening. No undocumented trust/permission change. Forward-only success does not pass. Also test custom API-client valid/missing-header behavior matches documented capture contract. |
| AC20 | Normal defaults, no optional keys/services, multiple synthetic sessions and streaming turns. Fifteen minutes; a longer run with eight concurrent fixture sessions is not an alpha gate. | All expected completed captures accounted for; no panic, unbounded error flood/backlog or unexpected paid calls; UI/MCP remain usable; queues drain and resource use stays within recorded configured limits. Report peak resources; this is bounded reliability evidence, not a scalability claim. |
| AC21 | Interrupt disposable DB, return upstream 429/500, cancel a stream, exhaust a quota-limited test data volume, then restore prerequisites. | Documented client/error behavior, observable partial/failed capture or degraded persistence, bounded retries/resource cleanup; no claim that failed writes are durable; restored system captures again and prior durable data remains. Record whether interrupted records can recover and any supported loss window. |
| AC22 | Capture dataset including external body/blob state and settings; follow documented consistent backup; restore to fresh DB/data root. Inject backup failure. | Matching IDs/counts/body checksums plus UI/MCP reads after restore. Required config/blob state preserved. Failed backup is nonzero and never labelled complete; secret-bearing backup remains private. |
| AC23 | Install frozen predecessor, populate data/config, back up, upgrade via real channel, interrupt/retry update, attempt incompatible downgrade. | Candidate retains data/config and new capture works; valid ledger; safe retry; incompatible older binary refuses before mutation and gives recovery instructions. Out of the alpha (does not gate); the first alpha has no predecessor. A later release tests the actual published one. |
| AC24 | Remove installed product with captures/backup and unrelated client config present; then reinstall. | Only owned binary/integration/service changes; captures/config/volumes/backups retained by default; unrelated client entries survive; reinstall reconnects history. Any explicit purge is separate and unambiguous. Out of the alpha (does not gate). |
| AC25 | Prepare twice; simulate missing publication/tap permission, failed draft upload and partial publication. Retry. | Preparation leaves tracked source untouched; preflight failures precede publication; retry consumes identical verified hashes; published tags/assets are not rewritten; no false completed release. |
| AC26 | New account installs from the actual published URLs/tap, not staging files. | Downloaded hashes equal qualified artifacts; AC01–AC07 pass from channel install. Homebrew and archive/download claims each need their own observation. |
| AC27 | Seed fake secrets and request content; run local version, read-only doctor and opt-in support bundle with daemon stopped/degraded/ready. Exercise legitimate idle and expected-but-missing capture states at threshold boundaries under #200's documented expectation policy. | Version needs no config/DB; doctor mutates nothing; required failures nonzero, optional absence explained. Capture-gap threshold/state are exposed in health, expected missing traffic affects health/UI as documented, legitimate idle is distinguished, and boundary tests cover both. Bundle contains only safe allowlisted metadata, no planted secret/content; user can inspect it and nothing uploads automatically. |

AC19 is a separately scheduled human compatibility run, using an authorized
account, not a provider credential in ordinary CI. CI exercises a fake client
and controlled upstream; those cannot substitute for the real-client result.
The current extractor requires `x-claude-code-session-id`. Do not invent an
identity or infer capture success from a provider response. For custom clients,
document/test the identity requirement and explicit uncaptured-forwarding state.

AC18: the app renders its own documentation and no roadmap appears. AC27: the capture-gap threshold, health and UI behavior and its boundary conditions need evidence; age alone is not proof that routing is bypassed.

For AC21, choose the documented capture-failure/forwarding policy before writing
assertions, then test it. Do not silently redefine data integrity to match a
failed run. Any supported loss window belongs in the release notes and UI
health; unexplained silent loss blocks the release. Apply disk faults only to
owned quota-limited fixtures, never the host filesystem or capture database.

## Public-content and feedback gates

| ID | Actions | Observable pass criteria |
| --- | --- | --- |
| PUBLIC01 | Inspect candidate binaries, embedded SPA/source maps/resources, archives, example config, diagnostics and download repository content. | No private captures/credentials or undeclared maintainer dependency ships; required licenses/notices present; distribution repository access matches the public destination. |
| PUBLIC02 | Inventory current tree and reachable history for proposed public source; scan candidates and manually review capture/screenshot material; rehearse selected clean export/sanitization. | Concrete reviewed publication tree/history, no unresolved private-content exposure, confirmed usable credentials rotated, source archive contents reviewed; destination and SHA mapping recorded. Applies before source visibility, independently of any binary release. |
| FEEDBACK01 | After a public release: independent users follow only the published guide and artifacts; collect installation and repeat-use feedback; ship a corrective update. | First outside install and feedback exist (not an alpha gate); intervention and support data are recorded without conversation uploads; update fixes a reported issue and passes predecessor/channel tests.  |


As of 2026-11-06 the source is public (`temet-ai/mike`, a collapsed export made by
the public-export step, which lives in the private repository) and PUBLIC01, PUBLIC02 and FEEDBACK01 have no verdict
recorded in this repository. The export's own checks (gitleaks, the private word list, no maintainer
paths, no dead links) are mechanical and do not stand in for either review.

## Who verifies what

Verification responsibility is split by area; an area's case is recorded in the ledger only from a real journey on the candidate.

| Area | Cases | Verification |
|---|---|---|
| Archive, installer, publication | AC01, AC08, AC25, AC26 | Archive/installer verifier; AC26 after an authorized publication |
| Database provider and migrations | AC02, AC09, AC10 | Disposable PostgreSQL and installed-provider verifier |
| Capture fidelity and the archive harness | AC03, AC20 | Synthetic capture and archive harness verifier |
| Packaged browser and onboarding | AC04, AC18 | Extracted-product browser verifier |
| MCP, client and diagnostics | AC05, AC17, AC19, AC27 | MCP/client and safe-diagnostics verifier |
| Local access policy | AC06 | Independent local-access security verifier |
| Lifecycle and instance isolation | AC07, AC14, AC15 | Lifecycle, restart and instance-isolation verifier |
| Configuration and literal input | AC11, AC12 | Configuration and literal-input verifier |
| Setup and mode transitions | AC13, AC16 | Mode-transition and failure-recovery verifier |
| Backup, upgrade and removal | AC21, AC22, AC23, AC24 | Maintenance/data-preservation verifier (AC23, AC24 outside the alpha) |
| Source gates | SG01, SG02, SG03 | Fresh-source gate verifier |
| Public content | PUBLIC01, PUBLIC02 | Distribution/source reviewer and operator approval |
| Feedback | FEEDBACK01 | External alpha user and feedback owner |

## Where the harness lives

`scripts/release-acceptance` verifies and extracts the exact candidate archives named by the
manifest and never falls back to a developer binary; its journeys are
`scripts/release-journey-mise` and `scripts/test-release-recovery-db`. Raw outputs use a new
private directory outside the checkout. Each case report contains ID, status, expected and
observed result, runtime input identity, command and exit status, elapsed time and evidence
locations. Failure logs are sanitized before retention. Cleanup is scoped to that run's
handles and labels, including cancellation paths. CI resource constraints are reported as
failed or not-run gates, not hidden by dropping tests or changing the candidate.
