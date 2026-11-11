.PHONY: test-area rc-protocol-recordrc-protocol-check lint-mods test-mods bootstrap preflight build build-release test test-db test-all test-p test-t test-repo-fast test-frontend-fast test-frontend test-e2e test-scripts test-repo test-timing lint fmt fmt-check deny check gate-quick install spa ast-grep-report mcp-fallback-report fidelity gate-tests doc-animations doc-recordings arch-check help

INSTALL_DIR     ?= $(HOME)/.local/bin
CARGO_TARGET_DIR ?= $(CURDIR)/target

## One-time setup: install the build accelerators (sccache, lld) that
## .cargo/config.toml requires. Idempotent — safe to re-run. A fresh checkout
## must run this once (or `brew install sccache lld`) before the first build.
bootstrap:
	@./scripts/lib/preflight-toolchain.sh

## Internal: verify the accelerators are present; auto-installs if missing.
## A prerequisite of every build/test target so a fresh checkout self-heals
## instead of failing cryptically. Set MIKE_SKIP_TOOLCHAIN_BOOTSTRAP=1 to skip.
preflight:
	@./scripts/lib/preflight-toolchain.sh

# Every compile/link path self-heals the toolchain first (order-only prereq so
# it never forces a rebuild). See scripts/lib/preflight-toolchain.sh.
build build-release test test-db test-all test-p test-t lint install: | preflight

# `mike` embeds frontend/build/ into the binary (crates/mike/build.rs +
# src/spa.rs, via rust-embed) — a real (non-order-only) prerequisite so
# `build`/`build-release`/`install` always ship a freshly built SPA rather
# than whatever was on disk from a stale prior run. Deliberately NOT a
# prerequisite of `test`/`lint`/etc. — those don't need Node at all.
build build-release: spa

## Build the SvelteKit SPA into frontend/build/ — the directory
## crates/mike/build.rs embeds into the `mike` binary via rust-embed
## (src/spa.rs). Re-run whenever frontend source changes; `build`,
## `build-release`, and `install` depend on this automatically.
spa:
	cd frontend && pnpm build

## Build mike-mcp and install to ~/.local/bin.
## Re-run after pulling changes to crates/mike-mcp or crates/mike.
install:
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) ./scripts/cx build --release --bin mike-mcp
	mkdir -p $(INSTALL_DIR)
	rm -f $(INSTALL_DIR)/mike-mcp   # fresh inode: cp over a previously-executed binary trips macOS's stale code-sign vnode cache -> silent SIGKILL (exit 137)
	cp $(CARGO_TARGET_DIR)/release/mike-mcp $(INSTALL_DIR)/mike-mcp
	@echo "✓ mike-mcp installed to $(INSTALL_DIR)/mike-mcp"
	@command -v mike-mcp >/dev/null 2>&1 \
		|| printf '⚠  %s is not in PATH — add to your shell profile:\n   export PATH="%s:$$PATH"\n' \
		   "$(INSTALL_DIR)" "$(INSTALL_DIR)"

build:
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) ./scripts/cx build --all

build-release:
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) ./scripts/cx build --all --release

## `claude plugin validate` on every mods/*/ (discovered by glob, so a mod that
## lands later is picked up). NOT RUN, not a failure, when `claude` is absent.
lint-mods:
	@if ! command -v claude >/dev/null 2>&1; then \
		echo "lint-mods: NOT RUN (claude is not on PATH) - not failing"; \
	else \
		fail=0; \
		for d in mods/*/; do \
			[ -f "$$d.claude-plugin/plugin.json" ] || continue; \
			if claude plugin validate "$$d" >/dev/null 2>&1; then echo "lint-mods: $$d PASS"; \
			else echo "lint-mods: $$d FAIL"; claude plugin validate "$$d"; fail=1; fi; \
		done; \
		exit $$fail; \
	fi

## `claude plugin test` on every mods/*/ that has *.test.ts or *.test.tsx files.
test-mods:
	@./scripts/test-install-mods >/dev/null || { ./scripts/test-install-mods; exit 1; }; echo "test-mods: install-mods PASS"
	@if ! command -v claude >/dev/null 2>&1; then \
		echo "test-mods: NOT RUN (claude is not on PATH) - not failing"; \
	else \
		fail=0; \
		for d in mods/*/; do \
			[ -f "$$d.claude-plugin/plugin.json" ] || continue; \
			[ -n "$$(find "$$d" -path '*/node_modules' -prune -o \( -name '*.test.ts' -o -name '*.test.tsx' \) -print | head -n 1)" ] || continue; \
			if claude plugin test "$$d"; then echo "test-mods: $$d PASS"; \
			else echo "test-mods: $$d FAIL"; fail=1; fi; \
		done; \
		exit $$fail; \
	fi

## Reviewed DB-free modules/harnesses. Check fixture drift before compilation.
test: test-mods
	./scripts/check-test-tiers
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) ./scripts/cx nextest run --all

## DB-backed scopes, including colocated unit and CLI/proxy tests. Each test provisions
## Postgres through its existing fixture. Needs MIKE_TEST_ADMIN_URL (see .env).
test-db:
	./scripts/check-test-tiers --require-db
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) ./scripts/cx nextest run --all --profile db

## Everything — fast tier + DB tier. What CI runs.
test-all: test-mods
	./scripts/check-test-tiers --require-db
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) ./scripts/cx nextest run --all --profile ci

## The tests of the areas this branch changed, in every tier.
test-area:
	./scripts/test-area

test-p:
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) ./scripts/cx nextest run -p $(p)

## Filter by source-module or test-fn name across ALL tiers, ignoring the
## default fast filter. `make test-t t=roundtrip` selects that module.
test-t:
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) ./scripts/cx nextest run --all --ignore-default-filter -E 'test($(t))'

## Everyday repository loop: Rust fast tier + frontend logic + script regressions.
test-repo-fast:
	@./scripts/test-suite fast

## Frontend SvelteKit sync + typecheck + unit and integration tiers.
test-frontend-fast:
	@./scripts/test-suite frontend-fast

## Full frontend gate: typecheck + logic + Storybook browser tests.
test-frontend:
	@./scripts/test-suite frontend

## Production SPA build + Playwright smoke test.
test-e2e:
	@./scripts/test-suite e2e

## Commit-gate and docs-agent regression tests.
test-scripts:
	@./scripts/test-suite scripts

## Every Rust, frontend, browser, and script test family.
test-repo:
	@./scripts/test-suite all

## Measure where every test suite spends its time: per-family phases (rust
## nextest, frontend, storybook, e2e, scripts) each with a per-test JUnit
## reporter on, aggregated by category (unit/integration/e2e/visual/scripts)
## under artifacts/<date>/test-timing/. Not a gate — scripts/test-timing
## --report-only re-aggregates without re-running; --no-db skips the DB tier.
test-timing:
	@./scripts/test-timing

## Rust structural lint and the rules and fleet text budgets. Lint checks the repository only;
## whether Claude Code's Remote Control still links to sessions is reported by
## the proxy's health status (the remote_control_link capability), and the two
## protocol scripts can be run by hand with `make rc-protocol-check`.
lint: lint-mods
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) ./scripts/lint-rust
	@./scripts/rule-checks/rules-budget.sh >/dev/null; rc=$$?; \
	if [ $$rc -eq 2 ]; then \
		echo "rules-budget: NOT RUN (reason printed above) - not failing lint"; \
	elif [ $$rc -ne 0 ]; then \
		./scripts/rule-checks/rules-budget.sh; \
		echo "rules-budget: the always-loaded .claude/rules set is over budget. Give a rule paths: frontmatter so it defers, or trim it."; \
		exit $$rc; \
	else \
		echo "rules-budget: PASS"; \
	fi
	@./scripts/rule-checks/fleet-text-budget.sh >/dev/null; rc=$$?; \
	if [ $$rc -eq 2 ]; then \
		echo "fleet-text-budget: NOT RUN (reason printed above) - not failing lint"; \
	elif [ $$rc -ne 0 ]; then \
		./scripts/rule-checks/fleet-text-budget.sh; exit $$rc; \
	else \
		echo "fleet-text-budget: PASS"; \
	fi
	@python3 -I ./scripts/rule-checks/fleet_text.py overlap >/dev/null; rc=$$?; \
	if [ $$rc -ne 0 ]; then \
		python3 -I ./scripts/rule-checks/fleet_text.py overlap; exit $$rc; \
	else \
		echo "fleet-text-overlap: PASS"; \
	fi

fmt:
	cargo fmt

## Fails on any file rustfmt would change; never rewrites, so an unformatted commit fails the gate.
fmt-check:
	cargo fmt --all -- --check

deny:
	cargo deny check || { echo "deny: advisory fetch failed; checking against the cached advisory database" >&2; cargo deny --offline check; }

## Full pre-completion gate: fmt-check + lint + ALL tests (fast + DB) + deny.
check: fmt-check lint test-all deny gate-tests

## Quick gate for a fleet's start and its pushed commits: format and compile only.
## The full gate (`check`) runs once, when the work is finished.
gate-quick: fmt-check
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) ./scripts/cx check --workspace --all-targets

## ast-grep compliance: compare ast-grep vs grep/LSP navigation calls from the proxy store.
## Optional args: make ast-grep-report SINCE="30 days"
SINCE ?= 7 days
ast-grep-report:
	@./scripts/ast-grep-compliance-report.sh "$(SINCE)"

## MCP fallback-adoption: compare mcp__mike__* calls vs proxy-q/psql/curl :9989 fallbacks.
## Optional args: make mcp-fallback-report SINCE="30 days"
mcp-fallback-report:
	@./scripts/mcp-fallback-report.sh "$(SINCE)"

## Run the inspection-fidelity self-check (scripts/fidelity-check): prints CLEAN or the hits.
fidelity:
	@./scripts/fidelity-check

## Run the commit-gate regression suite (fidelity-check scope + G3/G6 commit scope).
gate-tests:
	@./scripts/test-commit-gates

## Run a gate (default: check) and print a verdict that survives a pipe. Optional: make gate T=test-all
T ?= check
gate:
	@./scripts/gate $(T)

## Rebuild the animated SVG diagrams in docs/animations/ from docs/animations/build.mjs.
doc-animations:
	@node docs/animations/build.mjs

## Record the interactive demos in docs/animations/ playing their scripted run, as animated WebP for GitHub.
doc-recordings:
	@node docs/animations/record.mjs

## List Makefile targets with their descriptions.
help:
	@awk '/^## /{d=d?d" "substr($$0,4):substr($$0,4); next} /^[A-Za-z][A-Za-z0-9._-]*:/{split($$0,a,":"); printf "  %-18s %s\n",a[1],d; d=""; next} /^$$/{d=""}' Makefile

## Run the binary drift check in recording mode: updates the git-tracked
## scripts/rc-protocol-state/binary-last-verified.json. Operator-run; lint
## and the gate never record (mike#532).
rc-protocol-record:
	./scripts/rc-protocol-check-binary

## Run both Remote Control protocol drift scripts by hand: the strings in the
## installed Claude Code binaries, and the last hour of captured traffic.
## Not part of lint.
rc-protocol-check:
	./scripts/rc-protocol-check-binary --no-record
	./scripts/rc-protocol-check-capture
