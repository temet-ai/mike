---
title: Development
status: active
created: 2026-07-08
updated: 2026-11-20
summary: Scripts that exist, what the nightly job runs, and house rules for code in mike.
---

# Development

For the stack itself see [`GETTING_STARTED.md`](GETTING_STARTED.md).

## Scripts that exist

| Script | What it is |
|---|---|
| `scripts/cx` | cargo with `.env` and the checkout's `CARGO_TARGET_DIR` preloaded; sends `check`, `clippy`, `test`, `nextest` and `build` through mbx when it is installed |
| `scripts/sb-test` | the frontend test runner: svelte-check, unit, integration and Storybook browser tests |
| `scripts/lint-rust` | Clippy thresholds, suppression, architecture and log-field checks, the 500-line file cap |
| `scripts/sqlx-prepare` | regenerates `.sqlx/` |
| `scripts/nightly-tests` | the nightly job |
| `scripts/deploy-proxy.sh`, `scripts/start-proxy.sh`, `scripts/deploy-infra` | deploys |
| `scripts/install-cargo-sweep`, `mike-dev-cargo-sweep` timer | build-directory cleanup on the Mac and Linux |
| `scripts/check-test-tiers` | static classification of test sources; `.config/test-tiers.json` is the reviewed partition and `.config/nextest.toml` uses the same filters |

Storybook, the interactive workbench at `:6006`, is the systemd user unit `mike-stack-storybook`.

## What runs when

- There are no git hooks.
- Clippy and every test tier run nightly (`scripts/nightly-tests`, the `mike-dev-nightly-tests` systemd user timer, enabled where `.env` sets `MIKE_DEV_NIGHTLY_TESTS=1`). [`GITHUB_WORKFLOWS.md`](GITHUB_WORKFLOWS.md#rust-qualityyml) lists the rest.

## Test databases

Database-backed tests need an explicit `MIKE_TEST_ADMIN_URL` pointing at a disposable PostgreSQL instance with pgvector; the repository supplies no address or credentials. The fixture refuses the known live-capture instance and creates only its disposable databases.

The schema checksum test compares embedded migrations to `crates/mike/tests/schema/migration-baseline.json`, frozen at source `3cadea314`. Existing migration versions keep their checksum.

## House rules

- **Avoid `#[allow(...)]`** to silence clippy or dead-code warnings; fix the code instead.
  Test-helper modules under `crates/mike/tests/common/` carry `#![allow(dead_code)]` where a
  symbol is only used by some of the binaries that share the module. Complexity-policy
  suppressions are never allowed: cognitive complexity, argument count and function length are
  denied and a source scanner rejects attributes that could lower them. Rust files are capped at
  500 lines with no legacy allowlist.
- **No `.unwrap()` / `.expect()`** on `std::Option`/`std::Result`, enforced workspace-wide via
  `-D clippy::unwrap_used -D clippy::expect_used` in `.cargo/config.toml`'s
  `[target.'cfg(all())']` rustflags. Use `?` with `anyhow::Result`. The lint matches only the std
  methods; a differently-named `.unwrap()` (e.g. `chrono::LocalResult::unwrap()`) is not caught.
- **The design system is binding.** [Storybook's Foundations stories](http://localhost:6006/?path=/story/foundations-style-guide--core) (FUNK)
  govern every visual decision; [`frontend/CONVENTIONS.md`](frontend/CONVENTIONS.md)
  governs component code. No raw hex, no inline px, no hand-rolled dialogs.
- See [`../AGENTS.md`](../AGENTS.md) for agent routing, code-navigation tooling (ast-grep tier 1,
  LSP tier 2, grep as last resort), and the inspection-fidelity rule that binds any change
  touching capture, schema columns, REST projections, MCP tool shape, or rendering of inspection
  content.

## Links

- [`../README.md`](../README.md): project overview
- [`../AGENTS.md`](../AGENTS.md): project rules for coding-agent sessions
- [`GETTING_STARTED.md`](GETTING_STARTED.md): first-run setup
- [Mike Roadmap](https://github.com/orgs/temet-ai/projects/1)
