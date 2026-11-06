---
title: GitHub Actions Workflows
status: active
created: 2026-06-28
updated: 2026-11-06
summary: Reference for the GitHub Actions workflows in mike - which repository runs which, triggers, purpose, loop guards, and operational notes.
---

# GitHub Actions Workflows

Nine workflows live in `.github/workflows/` of the private development repository. The
public repository `temet-ai/mike` was created from a collapsed export that leaves out
`docs-agent.yml` and `deps-agent.yml`, so it carries seven. Actions is enabled on the
public repository and disabled on the private one ([`RELEASE_SOURCE_GATES.md`](RELEASE_SOURCE_GATES.md#hosted-ci-today)),
so nothing here runs privately: the two agents currently run nowhere, and the release
workflows run only on the public repository.

Four are CI gates (pull requests, and pushes to `main` for three of them); one is an
advisory usability gate on frontend PRs; two are release workflows (a tag-triggered
alpha build and a manual candidate source-gate run); two are autonomous agents (docs and
dependency advisories).

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
| `docs-agent.yml` ("Docs agent") | Push to `main` + `workflow_dispatch` | Analyses the diff and auto-updates affected docs (development repository only) |
| `deps-agent.yml` ("Deps agent") | Scheduled (Mondays 06:31 UTC) + `workflow_dispatch` | Caged Claude Code agent fixes advisories/deprecations/breaking bumps (coupled crate families, API breaks); a deterministic step re-verifies and opens the PR (development repository only) |

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

**Concurrency:** `rust-quality-${{ github.workflow }}-${{ github.ref }}` with `cancel-in-progress: true`. Only the latest push to a given ref runs; earlier runs are cancelled (the caller's workflow name is in the group so a candidate run cannot cancel a main run).

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

**Triggers:** PRs touching `frontend/src/**/*.svelte`, `*.ts`, `*.css`, any of the four usability
skills (`plain-language-microcopy`, `contextual-help-disclosure`, `invokable-page-tour`,
`ui-coherence-state-audit`), `scripts/usability-gate.sh`, or the workflow file itself.

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

---

## `docs-agent.yml`

The autonomous workflow. Everything below is how it was designed and debugged — useful context for a follow-up session.

### Triggers

```yaml
on:
  push:
    branches: [main]
  workflow_dispatch:
    inputs:
      diff_range:
        description: "Git diff range (e.g. abc123~5..abc123). Defaults to HEAD~1..HEAD."
        required: false
        default: ""
```

`workflow_dispatch` is the manual trigger — useful for testing or backfilling docs for an older range.

### What it does

1. **Checkout** — `fetch-depth: 0` (full history needed for arbitrary `git diff` ranges).
2. **Install Python 3.12** + `httpx` (from `scripts/docs-agent/requirements.txt`).
3. **Run `python scripts/docs-agent`** — the main logic:
   - Determines the diff range: uses `DIFF_RANGE` env if set, otherwise `HEAD~1..HEAD`.
   - Calls the LLM (OpenRouter, model `anthropic/claude-haiku-4-5`) to classify the diff into four boolean flags: `architecture`, `schema`, `guides`, `ui_help`.
   - Updates the relevant doc files based on those flags.
   - Commits the changes to a branch named `docs/agent-<short-sha>` and opens a PR via the GitHub REST API (`POST /repos/{owner}/pulls`).

### Python agent structure

```
scripts/docs-agent/
  __main__.py       # orchestration: diff → classify → update → PR
  client.py         # OpenRouter API client (httpx, model anthropic/claude-haiku-4-5)
  git_util.py       # git helpers: get_diff(), commit_and_push(), create_pr()
  requirements.txt  # httpx>=0.27
```

`create_pr()` uses `httpx.post` to the GitHub REST API directly with `GH_TOKEN`. The `gh` CLI is not used because it calls the GraphQL API, which is blocked in the remote execution environment.

### Required secrets

| Secret | What it is |
|---|---|
| `OPENROUTER_API_KEY` | OpenRouter API key — set in repo Settings → Secrets → Actions |
| `GITHUB_TOKEN` | Auto-provided by GitHub Actions — no setup needed |

### Infinite-loop guard

When the docs-agent PR is squash-merged, GitHub creates a commit whose message is the PR title. The PR title always ends with `[docs-agent]`. The job `if:` condition checks for this:

```yaml
if: ${{ github.event_name == 'workflow_dispatch' ||
        (!contains(github.event.head_commit.message, '[docs-agent]') &&
          github.event.head_commit.author.email != 'docs-agent@github-actions') }}
```

Three layers:
- **Squash merge** — the commit message contains `[docs-agent]` → job skipped.
- **Rebase merge** — individual commit message contains `[docs-agent]` → job skipped.
- **Belt+suspenders** — commit author email is `docs-agent@github-actions` → job skipped.
- **Plain merge** — the merge commit says "Merge pull request…" (no tag), but its diff contains only `.md` files → the LLM classify step returns all-false flags → script exits without opening another PR.

Always use **squash merge** when merging docs-agent PRs to keep the loop guard reliable.

### Permissions

```yaml
permissions:
  contents: write       # push a branch + commit docs changes
  pull-requests: write  # open the PR
```

These are job-level, so the default token for the rest of the repo remains unaffected.

---

## `release-alpha.yml` and `release-source-gates.yml`

`release-alpha.yml` ("Release alpha") runs when a tag matching `v*-alpha.*` is pushed, or on
manual dispatch (which builds and packages without publishing). Jobs: `validate`, then one
`build` per target (`ubuntu-22.04` for Linux x86_64, Apple Silicon on a macOS runner), then
`publish` on tags only. The steps, the Homebrew tap update, the secret it reads and the runs
so far are in [`RELEASING.md`](RELEASING.md#the-tag-triggered-alpha-workflow). The
development repository's copy builds on `macos-15`; the public repository's copy still names
the retired `macos-14`.

`release-source-gates.yml` ("Candidate source gates") is `workflow_dispatch` only, with
`contents: read`. It runs the offline routing regressions, then calls `rust-quality.yml`,
`storybook.yml` and `e2e.yml` as reusable workflows on the selected ref. See
[`RELEASE_SOURCE_GATES.md`](RELEASE_SOURCE_GATES.md#hosted-routing-contract).

There is no Dependabot auto-merge workflow any more, and Dependabot version updates are
disabled; `deps-agent.yml` is the only automated advisory path.

---

## `deps-agent.yml`

**Triggers:** scheduled `31 6 * * 1` (Mondays 06:31 UTC) +
`workflow_dispatch` with an optional `focus` input (an advisory id or crate name).
Development repository only; with Actions disabled there it does not run.

**Why it exists:** Dependabot's PR bot can't fix advisories that require bumping coupled crate
families in lockstep or absorbing a breaking API change at call sites (the worked example:
GHSA-w9wp-h8wv-79jx, where the `opentelemetry` family had to move together and `main.rs` needed a
manual `ExportConfig` → `with_endpoint` migration). This workflow runs a caged Claude Code agent to
do that work, then a deterministic step re-verifies and opens the PR.

**Security model:** the LLM step never touches `git`, `gh`, or a write token — its `claude_args`
allowlist is file edits plus a fixed set of `cargo` verbs (`check`, `build`, `clippy`, `deny`,
`update`, `fmt`, `tree`; no `cargo run`), and `GH_TOKEN` is not in its environment. It edits the
working tree and stops. A separate deterministic step then re-runs `cargo check`/`clippy -D
warnings`/`cargo deny check` itself (never trusting the agent's self-report), greps the diff for
a leaked API key, and only then commits to a `deps/agent-<sha>` branch and opens the PR via `gh`.

**What it does:**
1. Installs the pinned Rust toolchain + sccache + cargo-deny.
2. Gathers signals read-only (`cargo deny check` output, open Dependabot alerts via `gh api`,
   `deprecated` clippy warnings) into `/tmp/deps-signals.txt` — the only input the caged agent
   sees.
3. Runs the caged agent (`claude-code-action@v1`, routed through OpenRouter via
   `ANTHROPIC_BASE_URL`, reusing the existing `OPENROUTER_API_KEY` secret — same key as
   `docs-agent.yml`) to fix advisories/deprecations/breaking bumps and verify locally.
4. Deterministically re-verifies, secret-scans the diff, and opens the PR — or exits cleanly with
   no PR if the agent made no changes or the gate didn't pass.

**Required secret:** `OPENROUTER_API_KEY` (same one `docs-agent.yml` uses). Without it the
`fix-advisories` job logs a warning and every step after the key check is skipped.

---

## Repository Actions settings (important)

The workflows use external GitHub-hosted actions (`actions/checkout`, `dtolnay/rust-toolchain`, `taiki-e/install-action`, `Swatinem/rust-cache`, `pnpm/action-setup`, `actions/setup-python`, `actions/setup-node`).

**The repo must have Actions permissions set to "Allow all actions and reusable workflows"** (Settings → Actions → General → Actions permissions). If this is changed to "Allow local actions only" or a restricted allowlist, every workflow fails immediately with `startup_failure` (zero jobs start, zero log output). This is not an outage — it is a settings misconfiguration. (The public repository is set to "all actions" with Actions enabled. The private development repository has Actions disabled altogether, so its workflows do not start at all.)
