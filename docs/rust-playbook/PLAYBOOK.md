# The Rust Service Playbook

> Hard-won lessons for building a new Rust service, distilled from **temet-proxy** — a reverse proxy
> for Claude API traffic (two crates, axum, Postgres + sqlx, a SvelteKit inspection SPA, ~96k LOC,
> 160 commits). Every lesson is phrased to be **portable**: the general principle first, then the
> concrete temet-proxy instance as evidence. Copy this folder into a new repo and use it as a
> checklist of decisions to make on day one — most of the costliest lessons here are decisions that
> are cheap up front and expensive to retrofit.

## Purpose

This is not a tutorial and not API documentation. It is a **decision record turned forward**: the
patterns this project adopted, the anti-patterns it fought (often more than once), the tooling it
standardized on, and the incidents that taught it the difference. The dominant meta-theme across
every section is: **decide it on day one, or pay to retrofit it.** Crate boundaries, the database
schema, bounded memory, test isolation, compile-checked SQL, where heavy work runs — each was, at
some point here, a cheap up-front choice deferred into an expensive migration or an outage.

## How this playbook was built

It was assembled by a multi-agent pipeline run *through this very proxy*, which captured the build
traffic with byte-identity — so the process that wrote the playbook is itself an instance of the
playbook's observability lesson ([CS-5](case-studies.md#cs-agent-forensics)):

1. **Discovery** — 11 read-only agents swept the corpus (the `.claude/rules/` codified prohibitions,
   23 distilled memory lesson-files, 160 commits, the incident records, structured reviews, research
   spikes, the code, the schema, the tests, the tooling) and returned **206 candidate lessons**.
2. **Consolidation** — clustered into **17 investigation topics** mapped to 5 parts.
3. **Investigation** — 17 read-only agents verified each theme against source and wrote findings to
   disk (152 lessons, every citation self-checked).
4. **Adversarial verification** — 17 independent auditors **re-resolved every citation** without
   trusting the self-checks (596 citations, **95% confirmed first-pass**; the rest were line-precision
   slips, corrected), plus 3 cross-cutting critics confirmed **104/104 high-salience themes covered**
   and produced the reconciliation contract this document follows.
5. **Synthesis** — this backbone, written by hand; chapters expanded in parallel; one adversarial
   cohesion pass over the assembled whole.

The full working record lives in `artifacts/2026-05-30/playbook-build/` (git-ignored scratch).

## The governing rule: tense maps to ship-state

A playbook that launders an unshipped fix into a proven pattern is worse than no playbook. Throughout,
**SHIPPED** lessons are taught in the past/present; **DECIDED-NOT-SHIPPED** lessons are taught as "the
principle, validated by a real incident — implementation pending," never as running code. The biggest
DECIDED-NOT-SHIPPED items, flagged inline where they appear: the out-of-process embedding worker
([CS-3](case-studies.md#cs-embeddings-oom)), DB-minted identity + de-partitioning the high-volume
table, the concurrent dropped-write retry fix, and the structural `blocked` agent-state enum.

## Four threads that recur

These run through every part. Each lesson is tagged with the thread(s) it serves; they are principles,
not chapters.

1. **Source-of-truth discipline** — one canonical representation; everything else is a *rebuildable
   projection*. No column that is a slice of another column; no derived aggregate that can drift from
   its source; config has one authoritative resolver and the rest are views of it.
2. **Bound-it-before-you-need-it** — every buffer, channel, cache, task pool, and stream gets an
   explicit budget *at design time*, not after the OOM. Backpressure (429/503) is correct behaviour,
   not an error to hide.
3. **Deterministic enforcement over prose** — a compiler lint, a `CHECK` constraint, a foreign key, a
   parity test, or a hook beats a `MUST` in a document every time. Prose steers only probabilistically.
4. **Evidence over attestation** — "done" requires pasted command output or a running-system
   observation, not a sentence claiming success. This applies equally to code review and to the agents
   that write the code.

## Where "security" shows up

There is no standalone security chapter by design; the concern is woven in. Find it as: **byte-identity
of forwarded content** (ch. 2), **bounding untrusted hot-path inputs** (ch. 2 / ch. 9), **fail-fast
config validation** (ch. 14), **structural test isolation** (ch. 11), and the supply-chain / `unsafe` /
secrets recommendations the project has *not yet* adopted (Appendix A).

## Contents

**Part I — Foundations & Structure**
1. [Workspace & crate boundaries](parts/01-T01-workspace-crates.md)
2. [The hot path & passthrough discipline](parts/02-T02-hot-path-passthrough.md)

**Part II — Data & Persistence** *(the "decide the durable schema first" spine)*
3. [Plan for the database from day one](parts/03-T03-db-planning.md)
4. [Schema design & normalization](parts/04-T04a-schema-design.md)
5. [Migrations: expand-only + discovery-before-scope](parts/05-T04b-migrations-mechanics.md)
6. [The persistence boundary & write discipline](parts/06-T05-persistence-boundary.md)

**Part III — Runtime & Resource Safety**
7. [Async & concurrency discipline](parts/07-T07-async-concurrency.md)
8. [Memory & resource safety (+ heavy compute off-process)](parts/08-T06-memory-resource-safety.md)
9. [Error-handling architecture](parts/09-T08-error-handling.md)

**Part IV — Quality, Observability & Operations**
10. [Observability & inspection fidelity](parts/10-T09-observability-fidelity.md)
11. [DB-backed test isolation](parts/11-T10a-db-test-isolation.md)
12. [Test strategy: taxonomy, mocking, regression, parity](parts/12-T10b-test-strategy.md)
13. [Tooling & quality gates](parts/13-T12-tooling-quality-gates.md)
14. [Configuration architecture](parts/14-T11a-config-architecture.md)
15. [Operational safety & deploy](parts/15-T11b-ops-deploy-safety.md)

**Part V — Building with AI Agents**
16. [Agent-assisted dev: scope & delegation](parts/16-T13a-agent-scope-delegation.md)
17. [Completion integrity & the lean main turn](parts/17-T13b-lean-main-turn.md)

**Appendices**
- [Case studies — five incidents that earned their lessons](case-studies.md)
- [Appendix A — Gaps in our own practice (adopt from day one)](appendix-a-adopt.md)
- [Addendum B — What to do on temet-proxy (project-specific)](addendum-b-temet-proxy.md)
