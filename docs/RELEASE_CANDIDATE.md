---
title: Release candidate contract
status: active
created: 2026-10-13
updated: 2026-11-06
summary: What a release candidate is, what the 0.1.0-alpha.N alpha needs from it, who verifies which cases, and what a candidate never does; no qualification or publication approval.
---

# Release candidate contract

This is the evidence contract for a mike release candidate. It is not a release
approval. Where the release stands today is in
[`RELEASING.md`](RELEASING.md#where-things-stand-2026-11-06); the cases are in
[`RELEASE_ACCEPTANCE_MATRIX.md`](RELEASE_ACCEPTANCE_MATRIX.md).

## Source and scope

- A candidate is built from a clean, reviewed commit with `scripts/release.sh`. There is
  no candidate branch and no milestone stage: the commit is `HEAD` of the development
  repository, and its workspace version must already be the alpha version
  (`0.1.0-alpha.N`) in `Cargo.toml` and `Cargo.lock`.
- Each verification record names the actual candidate commit and artifact hashes.
  Earlier audit results and earlier candidates are not certification of later changes.
- The first targets are native Linux x86_64 (built on `ubuntu-22.04`, glibc 2.35 floor)
  and Apple Silicon macOS (built on `macos-15`). Persistent storage supports the
  Docker/BYO PostgreSQL contract only after the relevant provider and extension tests
  pass. Apple Silicon macOS requires its own native build and clean-install evidence.
  Remote exposure, autostart, unqualified platforms and unqualified optional
  integrations are not implied by a successful Linux test.
- The alpha bar is install and run, plus every case that can lose data (AC01, AC02,
  AC03, AC07, AC08, AC22). There is no pilot and no invited-user stage; AC23, AC24 and
  the upgrade, removal and reinstall journeys are not part of the alpha.

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
([route](RELEASING.md#the-route-to-a-release)). Binary delivery requires PUBLIC01 and the
public source requires PUBLIC02. Published-channel qualification and external feedback
are pending until a release is published and observed.

## Who verifies what

Separate verification responsibilities by area. The case that an area owns is recorded
in the ledger only from a real journey on the candidate.

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

Configuration owns one selected installation and resolved boot settings. Setup,
doctor, MCP registration, provisioning and lifecycle consume that contract rather
than discovering a second working-directory `.env`. Lifecycle owns daemon identity
and the complete private boot snapshot; setup must not adopt a healthy daemon
whose configuration or requested persistence mode differs.

## Acceptance responsibility

All 33 acceptance cases remain evidence-bearing; a checkbox or a helper test is
not an installed user journey. Each case records selected/executed/ignored/excluded
counts where applicable, command exit status, observed outcome, candidate SHA, and
relevant tool/platform/database/client versions. Reports exclude credentials and
captured user content. Outcomes are PASS, FAIL, NOT RUN, or DEFERRED with a reason.
A required NOT RUN or DEFERRED case prevents the alpha gate only if it is one of the
six alpha cases; it prevents qualification of anything that requires it.

Run every feasible gate locally and retain its own real exit status. Hosted Actions are
enabled on the public repository and disabled on the private development repository
([`GITHUB_WORKFLOWS.md`](GITHUB_WORKFLOWS.md)); a local pass does not imply a GitHub
check passed, and a hosted result is recorded as its own observation. Native macOS,
real external installations and external feedback need their actual environments or
participants.

## Baseline

Embedded SPA delivery, stock-safe Cargo configuration, capture-gap health, gate
signal/source-drift handling and additive SQLx metadata preparation already exist.
Preserve them and qualify their current combined behavior instead of implementing
them again. The setup/control TUI is delivered and now has seven steps, including
terminal backend selection; its input privacy, requested/stored/running mode
agreement and installed provisioning still need evidence under AC12, AC13 and AC16.

No artifact, platform, database version, external client or publication channel
is certified by this document.
