---
title: Mike public release readiness
status: active
created: 2026-09-17
updated: 2026-11-06
summary: The September 2026 release stocktake kept as history - verified blockers, supported installation proposal and journey list; the current state and the alpha bar are in RELEASING.md and RELEASE_ACCEPTANCE.md.
---

# Mike public release readiness

> **This is the 2026-09-17 assessment, kept as the record of why the work started.**
> Where things stand now is in [`RELEASING.md`](RELEASING.md#where-things-stand-2026-11-06):
> the public repository exists and `0.1.0-alpha.3` is published as a pre-release. The alpha bar is the six cases in
> [`RELEASE_ACCEPTANCE.md`](RELEASE_ACCEPTANCE.md); there is no pilot. The blocker
> table below is a September snapshot, not a current defect list.

**Decision on 2026-09-17: not ready for a public release.** The core product and
much of July's release foundation exist. The unfinished work is the complete
installation, configuration, storage, lifecycle, and upgrade experience on a
machine that does not belong to the maintainer. Several current paths fail even
before a user can capture their first request. Public source also needs a
privacy review before the repository's visibility changes.

This is an assessment of source at `3c27b002146a47890ac917210b914495f805d861`,
plus the small configuration fixes described below. It does not certify a final
release artifact. The audit's lane reports (installation, runtime, quality and
privacy, verification record, disposable database probe) are development records
and are not part of the public tree; their conclusions are summarised here.

## What happened to the July release goal

The July goal ("release mike") had closed the embedded SPA, removal of mandatory
Cargo accelerators from plain Cargo, the setup/control TUI, capability reporting,
public naming, funding and the Claude wrapper. Distribution was the open item: it
held a working-history record of Apple Silicon builds and packaging, and the
explanation that publication was waiting for approval or frozen CI was already
stale in September. Issue closure alone is not installation evidence.

GitHub queries on 2026-09-17 found a private repository with no tags, no GitHub
Releases, no release workflow and an unavailable tap, a release target that built
Apple Silicon only, and four failing Rust quality runs (the latest at
`cargo fmt --check`). That has changed: the repository, the tap, a tag-triggered
`release-alpha.yml` and a published alpha now exist (see
[RELEASING.md](RELEASING.md#where-things-stand-2026-11-06)).

## What is already usable

| Area | Evidence and current boundary |
| --- | --- |
| Proxy and inspection | Current debug binary forwarded a synthetic non-streaming request and exposed its captured request/response bodies after restart against disposable PostgreSQL 17. This smoke-mode proof must be repeated with the shipped archive and normal runtime. |
| Embedded browser UI | Existing developer binary served HTML and the same-origin health API outside the checkout without Vite. This was an older, dirty developer build, not the final audited source. |
| CLI and TUI | `mike`, `mike serve`, and `mike claude` exist. The wrapper can use the direct proxy without herdr or the forward sidecar. Setup has portability defects. |
| Persistent storage | Current debug build applied all 161 migrations to disposable PostgreSQL 17, then restarted with a consistent ledger, healthy persistence and the captured request retained. The external bootstrap script must stop competing with that migration owner. This did not exercise the advertised pg16 installer. |
| Optional integrations | Missing provider keys/embeddings produce capability states and permit basic ephemeral startup. Their full configured paths were not certified here. |
| MCP | Separate native bridge, core/full profiles, and tool enumeration exist. Registration must use the selected installation and actual inspect URL. |
| Terminal launch | herdr, tmux, and a terminal adapter exist. The latter depends on Hyprland; it is not a generic Linux desktop implementation. Several docs lag this code. |
| Frontend quality | Typecheck and 1,106 unit/integration tests in 73 files passed during this audit. Browser/Storybook gates were not run. |

The complete no-DB Rust default-profile probe reported **3,158 passed, 70 failed,
4 skipped**, with the existing-database checksum test explicitly excluded.
Sixty-three failures require test DB configuration, one requires absent
`.claude/old_rules`, and six are wrapper/proxy assertions needing follow-up.
Some reported passes skip DB work internally. This is evidence of an unreliable
test-tier boundary, not a passing release gate.

## Release blockers, in priority order

P0 blocks the first outside-user preview or public source release. P1 must be
resolved or explicitly excluded from the supported preview experience. Neither
label implies an unverified estimate of implementation effort.

| ID | Priority | Problem | Completion evidence |
| --- | --- | --- | --- |
| R01 | P0, public source | Tracked artifacts include raw request/response/system-prompt/tool captures and response cookie metadata. Maintainer paths and database URI patterns also occur across the tree. | Review/redact current contents and reachable history; run a secret scan; decide whether to sanitize history or publish a reviewed clean source history. Rotate any confirmed live credentials before exposure. No raw conversation export is included accidentally. |
| R02 | P0 | No usable public distribution channel; release script targets only Apple Silicon. Dry-run dirties formulae, current-version publish attempts an empty commit, and late tap checks can leave a partial publication. | Linux and macOS archives install without a compiler or checkout; prepare is repeatable; checksums work on another machine; publication preflights all destinations and can resume safely. |
| R03 | P0 | Setup writes cwd-relative `.env`, discovers config by walking directories, and calls a checkout-relative DB script. | Setup in directory A works when launching from B, a read-only cwd, and a service; one documented per-user config location; literal-safe, private, atomic writes; explicit env/CLI precedence. |
| R04 | P0 | Docker DB setup requires host `psql`; its fallback executes raw migrations without sqlx history. Native fallback also assumes roles/paths not established on a clean machine. | Docker-only setup works without host psql/sqlx; exactly one migrator owns schema/history; repeated setup/boot and a real capture/restart preserve data. |
| R05 | P0 | Trial → database setup can retain `PERSIST_ENABLED=0` and reuse an already-running ephemeral daemon. | Both mode transitions persist correctly; runtime health verifies the requested mode; a captured request survives the promised persistent restart. |
| R06 | P0 | Stop trusts numeric pidfiles and can signal an unrelated process after PID reuse; invalid values can become process-group/broad signal targets. | Reject zero and out-of-range PIDs; verify process identity/ownership; isolate concurrent starts and instances; stale-pid tests leave an unrelated scratch process alive. |
| R07 | P0 | Release gates do not prove a stranger can install/use the artifact; Rust baseline is red and some nominally fast tests use DB fixtures. | Fresh-clone CI passes with disposable DBs and complete sources; extracted-archive acceptance passes on supported platforms; no skipped DB work is reported as tested. |
| R08 | P1 | First-run input and MCP defects: `s` skips provider-key entry, DB credentials can be displayed, shell quoting changes literal passwords, MCP registration assumes PATH/default port. | Synthetic special-character credentials round-trip privately; key entry works; MCP handshake targets the installed sibling binary and configured URL. |
| R09 | P1 | Docs lead through the maintainer's Rust/Vite/Storybook workflow, and lack a complete upgrade, backup, restore, stop and uninstall contract. | A new user follows only the primary guide, captures a request, restarts, upgrades, restores a backup and removes the program without accidental data deletion. |
| R10 | P1 | Local-only API boundary is incomplete when SPA bind changes; automatic Claude trust and optional mutation behavior need explicit user-facing explanation. | All API-bearing listeners follow the same local-only policy; users can understand/configure trust and request/response mutation behavior. Remote access is not advertised as supported without authentication work. |

Additional details in the lane reports include missing `mike --version`, no
general installation doctor, incomplete child-process config propagation,
optional integration settings not applied immediately to a running daemon,
undeclared shell/tool requirements, and stale source-build instructions.

## Recommended first release contract

**Proposed support scope:** native Linux x86_64, explicitly tested on Arch, and
Apple Silicon macOS. This is the audit's working recommendation, not a claim
that either has a tested current release. Add a conservative Linux build
baseline and test its archive on Arch; building only on a rolling-release host
does not establish compatibility with older distributions. Add other
architectures only when their installation journeys run in the release matrix.

The user installs **prebuilt `mike` and `mike-mcp`**, runs `mike` for guided setup,
then uses `mike claude`, the browser UI, or a separately configured Claude
client. Normal operation must need no Rust, Node, pnpm, Storybook, checkout,
maintainer dotfiles, LAN database, herdr, or paid optional provider account.

Use one native release product with two install channels: a verified Linux
archive/download installer and macOS Homebrew. An AUR package can follow; it
does not need to delay a working Linux binary. Keep the full Docker/GHCR
application distribution optional, matching the July channel decision. Docker remains a supported way
to provide the database.

Offer persistent setup using either Docker/Compose with pgvector or an existing
compatible PostgreSQL database. Validate prerequisites and extensions before
declaring completion. If trial mode remains offered, describe its actual data
loss and feature limits and test its printed launch command. The current
installer defaults to trial but then recommends a launcher that requires a DB.

Make core capture, browser inspection, persistence, and MCP the required
installation acceptance surface. Test provider-assisted features and advanced
fleet/terminal/Remote Control integrations separately, with explicit
requirements. They must not prevent users from reaching their first capture.
This is a support/documentation boundary, not a request to silently change
feature defaults or remove existing functionality.

## Ordered execution plan

Each stage has a concrete exit gate. Complete these within the release effort;
do not reopen unrelated feature development as a prerequisite.

1. **Make the source shareable and checks trustworthy.** Owner: repository/
   security and test maintainers. Review tracked captures/history; remove
   personal configuration from distributed examples and tooling. Fix the
   missing `.claude/old_rules` dependency, formatting failures, fast-tier DB
   leakage, and CI trigger gaps. Exit: a fresh clone can run the documented
   gates using only declared tools and disposable services. This stage can
   run alongside first-run implementation; public visibility waits for it.
2. **Finish installed-user setup and lifecycle.** Owner: Rust + database.
   Address R03–R06 and the runtime portions of R08/R10. Establish config/data/
   log locations; package DB provisioning; keep embedded migration ownership;
   verify mode changes, MCP registration and process identity. Exit: an
   extracted binary reaches persistent capture from a clean HOME, independent
   of cwd, and repeated setup/start/stop is safe.
3. **Produce repeatable Linux/macOS artifacts.** Owner: release tooling.
   Address R02; build the SPA before embedding; validate archive contents and
   build identity; separate preparation from publication; add version/doctor
   support and supported-platform smoke jobs. Exit: the exact staged archives
   pass the matrix below without access to the source checkout.
4. **Complete user docs and outside-user rehearsal.** Owner: product/docs +
   independent verifier. Rewrite the primary quick start around installed
   product commands; document optional integrations, data locations, backup,
   upgrade and removal; repair contradictions in contributor instructions.
   Exit: someone following only those instructions reaches capture, MCP,
   restart and recovery without maintainer intervention.
5. **Publish a preview after the gates pass.** Use the reviewed artifacts and
   sanitized source decision, then verify real downloads/tap installation.
   Record supported platforms, known limitations and a reproducible support
   bundle with credential redaction. Treat the distribution work as done only
   after installation from the published channel works.

## Required release acceptance matrix

This is the September journey list. The current cases, their IDs and pass criteria
are in [`RELEASE_ACCEPTANCE_MATRIX.md`](RELEASE_ACCEPTANCE_MATRIX.md), and the
[local archive acceptance helpers](RELEASE_ACCEPTANCE.md) record them. For the
alpha, upgrade, uninstall and publication-retry rows below (AC23, AC24, AC25) do not
gate; the alpha bar is install and run plus the cases that can lose data.

Run against the final archives in fresh accounts/containers/VMs where appropriate,
with no inherited `.env`, provider credentials or production database access.
Use a synthetic local upstream for repeatable capture tests, and one separately
authorized real Claude session for compatibility validation.

| Journey | Required assertion |
| --- | --- |
| Arch x86_64, Docker/Compose only | Install → setup → capture → UI/API → MCP works; no host Postgres tools or compiler needed. |
| Apple Silicon macOS, supported Homebrew path | Same journey outside the checkout; package signatures/runtime dependencies work on another machine. Needs a Mac: the maintainer runs it through the Homebrew tap once a release exists. |
| Existing Postgres + pgvector | Validate access/extensions, migrate, capture, restart twice; history and captured data remain correct. |
| No optional services | Core journey works without OpenRouter, embedding server, herdr or sidecar; capability explanations are accurate. |
| Config and mode changes | Alternate cwd/ports, paths with spaces, literal credentials, trial ↔ persistent, and CLI/env precedence all behave as documented. |
| Failure handling | Missing Docker, bad DB auth, missing extension, occupied ports and startup timeout produce an actionable failure without claiming ready. |
| Lifecycle | Stale PID, double start, concurrent start, multiple instances and login/reboot preserve correct process ownership. |
| Upgrade and recovery | Upgrade from the preview predecessor preserves data/config; backup restores to a fresh instance; unsupported downgrade is diagnosed. |
| Uninstall | Binaries/integrations are removed predictably; user data is retained unless explicitly deleted. |
| Publication retry | Failed asset/tap upload can resume using the same verified artifacts without deleting/moving published tags. |

## Changes made during this audit

- Removed the committed maintainer `MIKE_TEST_ADMIN_URL` default from Cargo
  configuration. DB-backed tests now require an explicitly chosen test instance;
  CI already sets one. This does not remove historical credentials or isolate
  the separate existing-database checksum test.
- Restricted the local database Compose publication to loopback and made it
  honor `MIKE_LOCAL_DB_PORT`, matching the helper's connection URL. Validated
  rendered Compose configuration for the default and a custom port. This does
  not repair the helper's migration path, nor recreate any running container.
- Documented test-database isolation and the checksum-test caveat in
  [Development](DEVELOPMENT.md#test-database-isolation).
- Repaired this checkout's local Git hook configuration: both hook symlinks
  and `core.hooksPath` still pointed at the old Mac path. Reinstalled the
  repository's existing hooks and verified they resolve here. This is local
  checkout repair, not a portable hook-installer redesign.

No release, visibility change, history rewrite, live database migration,
production restart, or optional feature-default change was performed. The
assessment is complete; the release implementation gates above remain open.
