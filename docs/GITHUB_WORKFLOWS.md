---
title: GitHub Actions Workflows
status: active
created: 2026-06-28
updated: 2026-11-06
summary: Reference for the GitHub Actions workflows in mike - which repository runs which, triggers, purpose, loop guards, and operational notes.
---

# GitHub Actions Workflows

Seven workflows live in `.github/workflows/`. Actions is enabled on the
public repository `temet-ai/mike` and disabled on the private development repository
([`RELEASE_SOURCE_GATES.md`](RELEASE_SOURCE_GATES.md#hosted-ci-today)),
so nothing here runs privately: the workflows run only on the public repository.

Four are CI gates (pull requests, and pushes to `main` for three of them); one is an
advisory usability gate on frontend PRs; two are release workflows (a tag-triggered
alpha build and a manual candidate source-gate run).

---

## Workflows at a glance

| Workflow (display name) | Trigger | Purpose |
|---|---|---|
| `rust-quality.yml` ("Rust quality") | PR (Rust paths) + push to `main` + `workflow_dispatch` + `workflow_call` | Full Rust gate: fmt, lint policy, size and complexity lint, nextest, sqlx, cargo-deny |
| `storybook.yml` ("Frontend quality") | PR (frontend paths) + push to `main` + `workflow_dispatch` + `workflow_call` | Frontend advisory audit, typecheck, unit/integration/Storybook tests, Storybook build |
| `storybook-coverage.yml` ("Storybook coverage") | PR (component paths) | Enforces every new component ships a `.stories.svelte` |
| `e2e.yml` ("E2E") | PR (frontend paths) + push to `main` + `workflow_dispatch` + `workflow_call` | Playwright smoke tests |
| `ui-usability-review.yml` ("UI usability review") | PR (frontend UI paths) | Deterministic usability gate (svelte-check, layout-shift guard, grep self-checks) + advisory Claude-graded usability review |
| `release-alpha.yml` ("Release alpha") | Push of a tag `v*-alpha.*` + `workflow_dispatch` | Builds the Linux and Apple Silicon archives with `scripts/release.sh`, publishes a pre-release and updates the Homebrew tap; see [`RELEASING.md`](RELEASING.md#the-tag-triggered-alpha-workflow) |
| `release-source-gates.yml` ("Candidate source gates") | `workflow_dispatch` only | Offline routing regressions plus the three reusable gates on the selected ref; see [`RELEASE_SOURCE_GATES.md`](RELEASE_SOURCE_GATES.md) |

---

## `rust-quality.yml`

**Triggers:** pull requests touching the Rust, SQLx, toolchain or lint paths listed in the
file, every push to `main` (unfiltered, documentation-only commits included),
`workflow_dispatch` and `workflow_call` (both unfiltered).

**What it does:**
1. Spins up a Postgres 19 service container (pgvector image) and creates the CI database.
2. Installs the pinned Rust toolchain (`1.95.0`) with rustfmt + clippy.
3. Installs `sccache` (required — `.cargo/config.toml` sets it as the rustc-wrapper; the build fails without it).
4. Runs in order: `cargo fmt --check` → the Rust lint-policy self-test (`scripts/test-rust-lint-policy`) → size and complexity lint (`scripts/lint-rust`) → `scripts/check-test-tiers --require-db` and `cargo nextest` (fast and DB tiers) → `cargo sqlx prepare --check` → `cargo deny check`.

**Concurrency:** `rust-quality-${{ github.workflow }}-${{ github.ref }}` with `cancel-in-progress` set for pull requests only. On a pull request only the latest push runs. On `main` a running job finishes and the newest push queues behind it, so a busy branch still gets a verdict (the caller's workflow name is in the group so a candidate run cannot cancel a main run).

**External actions used:** `actions/checkout`, `dtolnay/rust-toolchain`, `taiki-e/install-action` (sccache, nextest, cargo-deny), `Swatinem/rust-cache`.

---

## `storybook.yml`

**Display name:** "Frontend quality".

**Triggers:** PRs that touch `frontend/**`, `scripts/sb-test`, the frontend audit and
test-suite scripts, or the workflow file itself; every push to `main`; `workflow_dispatch`
and `workflow_call` (the last two unfiltered).

**What it does:** Installs pnpm dependencies, runs the frontend dependency advisory audit
(`scripts/test-suite frontend-audit`, [policy](RELEASE_SOURCE_GATES.md#frontend-advisory-policy)),
installs Playwright Chromium, rejects new visual-only unit tests that duplicate sibling
Storybook coverage, runs the authoritative `scripts/sb-test` gate (SvelteKit sync,
svelte-check, unit, integration, and Storybook browser tests), then runs
`pnpm run storybook:build` as the static-build smoke check.

**Concurrency:** `storybook-${{ github.workflow }}-${{ github.ref }}`, cancel-in-progress.

---

## `storybook-coverage.yml`

**Triggers:** PRs that touch `frontend/src/lib/components/**` or the workflow file.

**What it does:** Finds every newly-added `.svelte` file (not a story, not under `ui/`) in the diff and checks whether a sibling `.stories.svelte` exists. Fails with `::error` annotations for each missing story. This enforces the one-component-one-story rule at the CI layer.

**No concurrency group** — it's cheap and stateless.

---

## `e2e.yml`

**Triggers:** PRs that touch `frontend/**` or the workflow file; every push to `main`;
`workflow_dispatch` and `workflow_call` (the last two unfiltered).

**What it does:** Installs pnpm + Playwright chromium (with OS deps) and runs `pnpm exec playwright test`.

**Concurrency:** `e2e-${{ github.workflow }}-${{ github.ref }}`, cancel-in-progress.

---

## `ui-usability-review.yml`

**Triggers:** PRs touching `frontend/src/**/*.svelte`, `*.ts`, `*.css`,
`scripts/usability-gate.sh`, or the workflow file itself.

**Two jobs:**
1. **`gate`** (always runs, blocking) — `scripts/usability-gate.sh` (deterministic grep
   self-checks for the binding rules: no truncation, no Tailwind `bg-[--foo]` miscompile), then
   `svelte-kit sync` + `svelte-check`, then the consolidated frontend
   architecture guards (`src/lib/frontendArchitecture.guard.test.ts`).
2. **`grade`** (advisory only, `continue-on-error: true`) — skipped on draft PRs and skipped
   entirely if no `ANTHROPIC_API_KEY` secret is set. When it runs, a headless Claude Code
   (`claude-sonnet-4-6`, read-only tools) diffs the PR against its merge-base, grades the changed
   frontend surface against the four usability skills as a rubric, and posts one sticky PR comment
   (marked `<!-- usability-grade -->`) with a verdict table and `file:line` findings. It never
   edits files or pushes.

**Concurrency:** `ui-usability-${{ github.ref }}`, cancel-in-progress.

**Setup:** add an `ANTHROPIC_API_KEY` repo secret to enable the `grade` job; the `gate` job runs
regardless.

The grep checks the `gate` job runs are written into `scripts/usability-gate.sh`, and the
`grade` job's four rubric dimensions are written into the workflow's prompt; neither reads a
skill file. The `grade` job is currently switched off in the workflow (`if: false`, with the
reason in a comment beside it).

---

## `release-alpha.yml` and `release-source-gates.yml`

`release-alpha.yml` ("Release alpha") runs when a tag matching `v*-alpha.*` is pushed, or on
manual dispatch (which builds and packages without publishing). Jobs: `validate`, then one
`build` per target (`ubuntu-22.04` for Linux x86_64, Apple Silicon on a macOS runner), then
`publish` on tags only. The steps, the Homebrew tap update, the secret it reads and the runs
so far are in [`RELEASING.md`](RELEASING.md#the-tag-triggered-alpha-workflow). The
Apple Silicon build runs on `macos-15`, the oldest Apple Silicon image GitHub still hosts.

`release-source-gates.yml` ("Candidate source gates") is `workflow_dispatch` only, with
`contents: read`. It runs the offline routing regressions, then calls `rust-quality.yml`,
`storybook.yml` and `e2e.yml` as reusable workflows on the selected ref. See
[`RELEASE_SOURCE_GATES.md`](RELEASE_SOURCE_GATES.md#hosted-routing-contract).

There is no Dependabot auto-merge workflow any more, and Dependabot version updates are
disabled.

---

## Repository Actions settings (important)

The workflows use external GitHub-hosted actions (`actions/checkout`, `dtolnay/rust-toolchain`, `taiki-e/install-action`, `Swatinem/rust-cache`, `pnpm/action-setup`, `actions/setup-python`, `actions/setup-node`).

**The repo must have Actions permissions set to "Allow all actions and reusable workflows"** (Settings → Actions → General → Actions permissions). If this is changed to "Allow local actions only" or a restricted allowlist, every workflow fails immediately with `startup_failure` (zero jobs start, zero log output). This is not an outage — it is a settings misconfiguration. (The public repository is set to "all actions" with Actions enabled. The private development repository has Actions disabled altogether, so its workflows do not start at all.)
