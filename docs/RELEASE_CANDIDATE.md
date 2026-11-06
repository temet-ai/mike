---
title: Release candidate contract
status: draft
created: 2026-10-13
updated: 2026-10-13
summary: Selected release implementation base, support scope, ownership, and local evidence requirements; no qualification or publication approval.
---

# Release candidate contract

This is the implementation and evidence contract for milestone
[release mike](https://github.com/temet-ai/mike/milestone/2), tracked by
[#270](https://github.com/temet-ai/mike/issues/270). It is not a release approval.

## Source and scope

- Candidate branch: `release/mike-local-2026-10-13`.
- Starting source: `3cadea314f7a2daf5495da03cac1735247f23822` on `main`.
- Starting tree: `a6b3ad2e0df75c0573cd04b62679f11d6c27a770`.
- Both shipped binaries inherit workspace version `0.1.0`. This is the current
  internal candidate version, not a newly selected public release number.
- Each verification record must name the actual candidate commit and artifact
  hashes. September audit results and the starting source are not certification
  of subsequent changes.
- Earlier fleet handoff branches are not candidate inputs unless their changes
  are already on the selected main or are separately reviewed into this branch.
- One cumulative draft pull request carries this release work. Implementation
  proceeds from easier independent slices to the dependent runtime journeys.

The first qualification target is local-only Linux x86_64. Persistent storage
supports the Docker/BYO PostgreSQL contract only after the relevant provider and
extension tests pass. Apple Silicon macOS requires its own native build and
clean-install evidence. Remote exposure, autostart, unqualified platforms and
unqualified optional integrations are not implied by a successful Linux test.

## Preparation and distribution

Preparation writes ignored `dist-out/` staging only. A package must identify
both `mike` and `mike-mcp`, their shared version, source SHA, target, checksums,
runtime requirements, license and reviewed contents. It must not rewrite tracked
versions or formulae, publish tags, upload files, or change repository visibility.

The two native binaries remain Python-independent. The `mike` archive additionally
ships the optional `mike-maintenance` Bash launcher and `mike-maintenance.pyz`
standard-library bundle, each hash-bound in the manifest. Python 3.11+ and Bash
are required only for this local install/recovery tool. The kit works from an
extracted archive or its immutable installed generation without a source checkout;
it does not download dependencies or confer runtime/platform qualification.

The destination is the public `temet-ai/mike` repository, the version is the next
`0.1.0-alpha.N` and the channels are the `temet-ai/homebrew-mike` tap and mise
([route](RELEASING.md#the-route-to-a-release)). Nothing is published by this work.
Binary delivery requires PUBLIC01 and the public source requires PUBLIC02.
Published-channel qualification and external feedback are pending until the
publication is separately authorized and observed.

## Ownership and interfaces

The release implementation owner coordinates all packages in the draft PR;
domain ownership and separate verification responsibilities are:

| Packages | Implementation responsibility | Required independent review |
| --- | --- | --- |
| WP00 / #271, umbrella / #270 | Release contract and evidence ledger | Candidate/source reconciliation |
| WP01 / #272 | Deterministic local gates and disposable test fixtures | Test selection and failure propagation |
| WP02 / #273 | Local HTTP/WebSocket access policy | Security and rejected-event regression |
| WP03 / #274 | Selected config, private paths, import and atomic save | Literal round-trip and precedence |
| WP04 / #275 | Installed Docker/BYO provider and migration readiness | Disposable database qualification |
| WP05 / #63 | Instance identity, ownership and graceful lifecycle | Foreign-process and race cases |
| WP06 / #50 | Setup orchestration and truthful mode transitions | Installed TUI and recovery journeys |
| WP07 / #278 | Version, doctor, MCP registration and client connection | Real stdio and clean-client checks |
| WP08 / #279, #3 | Endpoint/project onboarding and packaged help | Packaged browser navigation |
| WP09 / #51 | Pure preparation, installer and qualified-byte publication | Artifact and installed-channel checks |
| WP10 / #280 | Archive acceptance harness | Separate verifier consumes prepared bytes |
| WP11 / #281 | Consistent backup, restore, upgrade and removal | State comparisons and failure rehearsal |
| WP12 / #282 | Artifact rights/privacy and source exposure review | Separate distribution approval |
| WP13 / #283 | Safe feedback kit and allowlisted diagnostics | Outside installation and repeat feedback |

Configuration owns one selected installation and resolved boot settings. Setup,
doctor, MCP registration, provisioning and lifecycle consume that contract rather
than discovering a second working-directory `.env`. Lifecycle owns daemon identity
and the complete private boot snapshot; setup must not adopt a healthy daemon
whose configuration or requested persistence mode differs.

## Acceptance responsibility

All 33 acceptance cases remain evidence-bearing; a checkbox or a helper test is
not an installed user journey.

| Cases | Primary package | Verification responsibility |
| --- | --- | --- |
| AC01, AC08, AC25, AC26 | WP09 | Archive/installer verifier; AC26 after authorized publication |
| AC02, AC09, AC10 | WP04 | Disposable PostgreSQL and installed-provider verifier |
| AC03, AC20 | WP10 | Synthetic capture fidelity and archive harness verifier |
| AC04, AC18 | WP08 | Extracted-product browser verifier |
| AC05, AC17, AC19, AC27 | WP07 | MCP/client and safe-diagnostics verifier |
| AC06 | WP02 | Independent local-access security verifier |
| AC07, AC14, AC15 | WP05 | Lifecycle, restart and instance-isolation verifier |
| AC11, AC12 | WP03 | Configuration and literal-input verifier |
| AC13, AC16 | WP06 | Mode-transition and failure-recovery verifier |
| AC21, AC22, AC23, AC24 | WP11 | Maintenance/data-preservation verifier |
| SG01, SG02, SG03 | WP01 | Fresh-source gate verifier |
| PUBLIC01, PUBLIC02 | WP12 | Distribution/source reviewer and operator approval |
| FEEDBACK01 | WP13 | External alpha user and feedback owner |

Each case records selected/executed/ignored/excluded counts where applicable,
command exit status, observed outcome, candidate SHA, and relevant tool/platform/
database/client versions. Reports exclude credentials and captured user content.
Outcomes are PASS, FAIL, NOT RUN, or DEFERRED with a reason. Required NOT RUN and
DEFERRED cases prevent qualification of the stage that requires them.

GitHub Actions usage is exhausted. Run every feasible gate locally and retain its
own real exit status. Do not rerun hosted workflows. Hosted trigger/permission
execution is explicitly NOT RUN; a local pass does not imply a GitHub check passed.
Native macOS, real external installations and external feedback need their actual
environments or participants, not necessarily paid GitHub CI.

## Reconciled baseline

Embedded SPA delivery (#47), stock-safe Cargo configuration (#48), capture-gap
health (#200), gate signal/source-drift handling (#247/#248), and additive SQLx
metadata preparation (#263) already exist. Preserve them and qualify their current
combined behavior instead of implementing them again.

#50 has been reopened on GitHub because its September WP06 hardening checklist remains
unfulfilled. Its historical TUI delivery is real; input privacy, requested/stored/
running mode agreement and installed provisioning still need evidence. The setup
now has seven steps including terminal backend selection. Release completion must
follow those acceptance conditions rather than treating earlier TUI delivery as complete hardening.

No artifact, platform, database version, external client or publication channel
is certified by this baseline document.
