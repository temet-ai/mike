# Appendix A — Gaps in our own practice (adopt from day one)

> **These are NOT evidence-backed lessons from temet-proxy.** They are domains a general "build a new
> Rust service" playbook needs that this project — a single reverse-proxy/observability service —
> never exercised, or named as a known hole and never closed. They carry no in-repo citations because
> there is nothing to cite. Treat them as the recommended day-one checklist this project *should have
> had*. Where the repo explicitly flags the gap (CI), that is called out — it is the strongest case
> for adopting the item before you have the same hole.

## A.1 — Dependency & supply-chain auditing *(strongest omission)*

The project has **zero** dependency-risk tooling — no `cargo-deny`, no `cargo-audit`/RUSTSEC, no
license policy — despite pulling axum, tokio, sqlx, reqwest, rmcp, and (the OOM culprit) fastembed/ort
across 40+ transitive crates. A new service needs a dependency-vetting gate from the first commit.

- `cargo deny check` — advisories + license policy + banned crates + source allowlist, in CI.
- `cargo audit` — RUSTSEC advisory scan (or fold into `cargo deny`).
- A written policy for *adding* a dependency (pairs with ch. 1's "pin shared deps once in
  `[workspace.dependencies]`" and a two-implementations-before-you-generalize rule).

```toml
# deny.toml — minimal starting point
[advisories]
yanked = "deny"
[licenses]
allow = ["MIT", "Apache-2.0", "BSD-3-Clause", "Unicode-3.0"]
[bans]
multiple-versions = "warn"
```
*(The global tooling mentions `grype` for Docker images — that is image scanning, a different layer.
Do not conflate it with crate auditing.)*

## A.2 — A real CI pipeline *(the repo names this gap)*

Chapters 13 and 15 both flag that `.github/workflows/` carries **no Rust lint/test/sqlx gate** (it
holds only Storybook workflows): every Rust quality gate (`clippy -D warnings`, `nextest`,
`sqlx prepare --check`, the proposed `cargo deny`) exists only in `make` and runs only when a human or
agent remembers to. Quality gates that aren't merge-blocking are suggestions. (See ch. 15 §9 for the
exact current state.) Close the gap this repo left open:

```yaml
# .github/workflows/ci.yml — the gates that must block merge
jobs:
  check:
    steps:
      - run: make check                              # fmt + clippy -D warnings + build
      - run: cargo sqlx prepare --workspace --check  # schema drift gate (offline cache fresh)
      - run: cargo deny check                         # supply chain (A.1)
      - run: cargo nextest run --all
```
Add a caching layer and an MSRV matrix. The principle (ch. 13's thread *deterministic enforcement over
prose*) only fully lands once the gates run on a machine no one can skip.

## A.3 — `unsafe` policy

Chapter 9 bans `unwrap`/`expect` via deny-lints but says nothing about `unsafe`. The app code appears
to be unsafe-free — but absence by luck is not a documented policy. State one:

```rust
#![forbid(unsafe_code)]   // crate root; or deny + a reviewed, // SAFETY:-commented exception list
```
Track unsafe surface in dependencies with `cargo geiger`. Pairs with ch. 9's deny-lint discipline.

## A.4 — Documentation practice (rustdoc & doctests)

The project has rich *operating* docs (ARCHITECTURE.md, SCHEMA.md, the rules files) but no rustdoc
discipline. A library/service playbook should require:

- `///` doc comments on every public item; `#![warn(missing_docs)]` at the crate root.
- Runnable **doctests** as tested examples (`cargo test --doc`).
- `cargo doc --no-deps` built in CI to catch broken intra-doc links.

This reinforces ch. 1's "document each `pub` item with who reaches it" — currently a visibility lesson
with no doc-tooling behind it.

## A.5 — Secrets handling

Chapter 14 layers config and notes that `print-config --shell` redacts the DB password — but there is
no first-class secrets discipline. Promote redaction from an incidental behaviour to a rule:

- Secrets never appear in logs, config dumps, or git. Prefer env-var or a secret store over file config
  for credentials.
- Consider the `secrecy` crate (wraps secrets so they don't `Debug`/`Display`) and `zeroize` for
  in-memory wiping.
- Make redaction a tested property of every config-dump surface, not a one-off.

## A.6 — Benchmarking & performance-regression gating

Chapter 2 establishes "the hot path must never degrade," and the project diagnoses performance
*reactively* (DB audits, `EXPLAIN ANALYZE`). It has no *proactive* guard. A service with a latency
contract should add:

- `criterion` micro-benchmarks on the hot path (forwarding, the SSE tee, the matcher).
- A load/latency test of the proxy path; a perf-regression threshold in CI for the critical route.

A benchmark *guards* the contract that ch. 2's correctness tests only *assert*.

## A.7 — Health / readiness / liveness taxonomy

The repo has the pieces — `/api/health`, a 120 s startup readiness gate, admission throttling — but
scattered across the memory, ops, and deploy chapters, never consolidated. A deployable service (esp.
under k8s-style orchestration) should state one model outright:

- **Liveness** — is the process up? (restart on fail)
- **Readiness** — can it serve? (gate traffic on the migration/readiness window; admission-throttle
  under memory pressure rather than failing liveness)
- **Startup** — long boot work bounded by a deadline, distinct from steady-state liveness.

## A.8 — Also needed for a deployable-service playbook *(brief mentions)*

- **Release engineering** — semver discipline for any published crate surface (ch. 1's `pub`/`pub(crate)`
  ladder is a semver concern with no semver lesson attached), `CHANGELOG.md`, git tags, and
  `cargo-release`/`release-plz` if anything is published.
- **Containerization** — a multi-stage Dockerfile with `cargo-chef` layer caching, a distroless/minimal
  base, a non-root user, and image-size discipline. Lower priority for a localhost tool; required for a
  service you ship as an image.
