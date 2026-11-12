---
title: Development
status: active
created: 2026-07-08
updated: 2026-11-09
summary: Dev loop commands and house rules for contributing code to mike.
---

# Development

For someone about to write code or send a PR against this repo. If you haven't got the stack
running yet, start with [`GETTING_STARTED.md`](GETTING_STARTED.md) — this doc assumes `mike`,
the inspect API, and the SPA already run locally and covers the loop you use while changing code.

## Dev loop

Make the change, run one narrow check of what you changed, commit. The pre-push hook runs
fmt, clippy and `scripts/lint-rust`, so run `git push` in the background; CI grades the
rest. Do not run them yourself per change. `scripts/test-area` runs the tests of every area
your branch changed, and [`GITHUB_WORKFLOWS.md`](GITHUB_WORKFLOWS.md#rust-qualityyml) says
what runs when.

| Changed | The one check |
|---|---|
| Rust | `scripts/cx check --lib -p <crate>`, or the one test: `scripts/cx nextest run -p mike --test <harness> -E 'test(<name>)'` |
| Frontend | `scripts/sb-test --skip-typecheck <filter>` |
| A script | the one `scripts/test-<name>` you touched |

The integration-test sources compile through a handful of harnesses (`proxy`, `integration`,
`e2e`, `cli`, `schema`, `reminder_on_token` and the `*_contracts` / `release_*` targets in
`crates/mike/Cargo.toml`), so a filter picks one concern.
`scripts/test-suite fast|all` exist for the landing gate, not the inner loop.

Other useful entry points:

- `scripts/cx <cargo args>` — cargo with `.env` and the checkout's own `CARGO_TARGET_DIR` preloaded, from
  any working directory (replaces manually sourcing `.env` first).
- Makefile shortcuts: `make test` (reviewed DB-free scopes) /
  `make test-db` (all required-DB scopes, including colocated unit and CLI/proxy tests) /
  `make test-all` (everything) / `make test-p p=<crate>` / `make test-t t=<filter>` (all tiers,
  matching source-module-qualified names as well as test function names).
- Never `cargo clean` — it discards the shared build cache and forces a cold
  rebuild of every harness. If a build looks wrong, fix the cause.

When mbx is installed, `scripts/cx` sends `check`, `clippy` and `test` through it into
`target/mbx-<session>`, and identical compiled units from any earlier mbx build on the machine
are restored instead of rebuilt. `build`, `nextest`, `scripts/sqlx-prepare` and any named
`CARGO_TARGET_DIR` stay on plain cargo; `MIKE_BUILD_CACHE=cargo` or `=mbx` overrides the choice,
and plain cargo is refused in an `mbx-*` directory. mbx is never combined with sccache. The
working rule is in [`AGENTS.md`](../AGENTS.md). What it does, the three traps and the trial scripts are in
[`scripts/mbx-trial/README.md`](../scripts/mbx-trial/README.md).

### Build disk cleanup on macOS

Run `scripts/install-cargo-sweep` to install or update the Mac's six-hour build
cleanup job (`--dry-run` previews the installation). It copies a versioned
runtime outside the checkout, so changing branches does not change the installed
cleaner. It migrates the old shared-target launcher to discover build directories
under `~/projects` and `~/.cache`, including worktrees. Existing interval and size
settings are preserved; a new installation uses a 25 GiB per-directory cap and
reports failure if free space remains below 30 GiB.

The LaunchAgent runs independently of Mike's working directory. An installed
Mike started from its state directory cannot run checkout-relative scheduler
scripts; an enabled `cargo_sweep` setting alone does not prove cleanup is running.
Check `launchctl print gui/$(id -u)/com.dawiddutoit.cargo-sweep` for the last exit
status, and `~/.local/state/mike/cargo-sweep/sweep.log` for the directories and sizes
actually swept. The installer keeps backups alongside the log.

The cleaner preserves compiler objects referenced by current macOS binaries'
debug maps. Removing every `*.rcgu.o` indiscriminately leaves binaries runnable
but can remove their debugger information. Busy profiles are skipped under Cargo
locks; an over-cap or failed cleanup is reported as a failure.

A finished checkout's build folder is emptied, keeping only its Cargo lock files:
a worktree whose branch is merged or gone (two hours after its last build), a
detached checkout whose commit is on main, or a folder no project owns (a day
after its last build), and a build-dir whose checkout is gone. `scripts/cx` also
sweeps its own folder after a build that leaves it over the cap.

### Test database isolation

Database-backed tests require an explicit `MIKE_TEST_ADMIN_URL`. The repository
must not supply a maintainer's database address or credentials. Point it at a
**disposable PostgreSQL instance with pgvector**; the test role must be able to
create databases. For example, after provisioning your own local test instance:

```bash
export MIKE_TEST_ADMIN_URL=postgres://postgres:postgres@127.0.0.1:5432/postgres
make test-db
```

This is an example endpoint, not a provisioned service. `scripts/cx` sources the
checkout's `.env` without overriding already-exported values, including an
explicitly empty value. Inspect that file before relying on an unset variable;
do not point test tooling at a capture database.
A selected test that needs a database fails when the variable is absent or blank;
missing infrastructure is not a passing assertion. CI provisions its own pgvector
service and sets the variable. The fixture refuses the known live-capture
instance and creates only its disposable databases.

The fast schema checksum test compares embedded migrations to the committed
`crates/mike/tests/schema/migration-baseline.json` frozen at source `3cadea314`. It never
searches for configuration, reads `MIKE_DATABASE_URL`, or opens a database.
Existing migration versions must keep their checksum; add a new migration rather
than regenerating this baseline to accept a historical edit. This is an internal
source baseline, not evidence of an upgrade from a published predecessor.

The `integration::migration_checksums` DB-tier test checks the complete ledger of
an explicitly provisioned disposable database. To inspect a real installation's
ledger, use the explicit read-only `mike --config /absolute/path/config.toml
migrate-check` command instead of pointing a test fixture at it.

The reviewed partition is `.config/test-tiers.json`; `.config/nextest.toml` uses
the same complement filters. Entire Mike modules with fixture consumers and the
proxy harness conservatively belong to DB, including their pure tests. New Mike
modules, CLI modules and harnesses default to DB until audited for fast. A source
scanner rejects known fixture or connection primitives in audited fast scopes;
the one missing-fixture contract exception is fingerprinted so changes demand a
new review. Make and the full Rust CI gate run this check before tests.

An isolated no-DB probe can run without a checksum exclusion:

```bash
env -u MIKE_TEST_ADMIN_URL -u MIKE_DATABASE_URL -u DATABASE_URL \
  SQLX_OFFLINE=true cargo nextest run --workspace --profile default
```

`scripts/check-test-tiers --json` records per-source classification, fixture
markers, test attributes and ignore attributes. Those are **static source
counts**, not compiled test counts. Never report static counts as tests run.
The [nextest filter syntax](https://nexte.st/docs/filtersets/reference/) uses exact
binary names and anchored module-name regexes here.

`make test-db` and `make test-all` require an explicitly exported nonblank admin
URL before building. Directly selected fixture consumers independently fail
closed if it is missing. These checks do not certify a full green release gate
or the predecessor upgrade journey; those still require actual execution.

### Frontend

Run from the **repo root**:

```bash
scripts/sb-test
```

This is the single frontend gate: SvelteKit sync + svelte-check, then unit and
integration tests with normal Node parallelism, then the Storybook browser
tests serially, in batches by area (`--list-batches`, `--batch NAME`). Nothing
stops early: the run ends with every failure and stall in one list, and logs stay
in `frontend/test-results/sb-test/`. A pass silent for three minutes is stopped,
and its unfinished files run one at a time so the stuck one is named. Logic tests run once even for a multi-theme Storybook pass, and
only the browser tier pays the serialization cost. The package test scripts
delegate to this runner; direct Vitest commands are reserved for watch mode.

The runner requires Python 3's standard library on Linux/macOS. A per-user
kernel lock in `/tmp` serializes complete invocations across checkouts, including
those with different temporary or config directories. Each run creates its own
process group and cleans only that group's browser/tool descendants on exit or
interruption. It never kills unrelated processes by browser or Vitest name.
Waiting runs can be interrupted without stopping the active run. Interrupted
runs exit nonzero with an explicit final `FAIL` verdict; they cannot qualify a
candidate. The lock is not a source freeze: the gate's input fingerprint checks
remain necessary.

Inner-loop forms (one per change):

```bash
scripts/sb-test --unit --skip-typecheck format
scripts/sb-test --integration cacheIdb
scripts/sb-test --storybook StatusIndicators
```

`--skip-typecheck` is intentionally not a gate. Other flags include `--theme N`,
`--themes[=a,b]`, and file/test filters (quote bracketed filters, for example
`scripts/sb-test --storybook 'routes/steering/[id]/page.stories'`).

Storybook, the interactive workbench at `:6006`, runs as the systemd user unit `mike-stack-storybook`
(`systemctl --user start mike-stack-storybook`; without systemd: `cd frontend && pnpm run storybook`)
for manual component development — that's a dev server, not a test command, so it's outside
`scripts/sb-test`'s scope.

## House rules

- **Avoid `#[allow(...)]`** to silence clippy or dead-code warnings — fix the code instead.
  This is a house-rule default, not a blanket clippy-enforced ban: real exceptions exist on
  both sides. Test-helper modules under `crates/mike/tests/common/` carry
  `#![allow(dead_code)]`/`#[allow(dead_code)]` where a symbol is only used by some of the
  binaries that share the module. Production code carries four: a module-level
  `#![allow(dead_code)]` in `src/resolvers/manifest.rs` and one item-level
  `#[allow(dead_code)]` each in `src/store/wiring.rs`, `src/store/persistence/lessons/mod.rs`
  and `src/store/persistence/lessons/writes.rs`.
  Complexity-policy suppressions are not exceptions: `scripts/lint-rust` denies cognitive
  complexity, excessive arguments, excessive function length, and Clippy's broader complexity
  group, while a source scanner rejects attributes that could lower them. Those functions must
  be refactored instead. Rust files are also capped at 500 lines with no legacy allowlist.
- **No `.unwrap()` / `.expect()`** on `std::Option`/`std::Result` — enforced workspace-wide (not
  only `src/`) via `-D clippy::unwrap_used -D clippy::expect_used` in `.cargo/config.toml`'s
  `[target.'cfg(all())']` rustflags, which cargo unions into every compiled target including
  `tests/`. Use `?` with `anyhow::Result` instead. Note the lint only matches the std
  `Option`/`Result` methods — a differently-named `.unwrap()` (e.g.
  `chrono::LocalResult::unwrap()`) isn't caught, so don't rely on clippy alone if you're calling
  an unwrap-like method from another type.
- **The design system is binding.** [Storybook's Foundations stories](http://localhost:6006/?path=/story/foundations-style-guide--core) (FUNK —
  open them in a browser) govern every visual decision; [`frontend/CONVENTIONS.md`](frontend/CONVENTIONS.md)
  governs component code. No raw hex, no inline px, no hand-rolled dialogs.
- See [`../AGENTS.md`](../AGENTS.md) for the full project rules — agent routing, code-navigation
  tooling (ast-grep tier 1, LSP tier 2, grep as last resort), and the inspection-fidelity rule
  that binds any change touching capture, schema columns, REST projections, MCP tool shape, or
  rendering of inspection content.

## Links

- [`../README.md`](../README.md) — project overview
- [`../AGENTS.md`](../AGENTS.md) — full project rules for coding-agent sessions working on this repo
- [`GETTING_STARTED.md`](GETTING_STARTED.md) — first-run setup: prerequisites, database, first run
- [Storybook Foundations](http://localhost:6006/?path=/story/foundations-style-guide--core) — FUNK design system, binding for all visual work
- [`frontend/CONVENTIONS.md`](frontend/CONVENTIONS.md) — frontend component conventions
- [Mike Roadmap](https://github.com/orgs/temet-ai/projects/1) — what's planned next (GitHub Project)
