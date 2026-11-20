# Case Studies — five incidents that earned their lessons

> These five incidents are referenced from multiple chapters. Each is told **once, here**, in full.
> A chapter that builds on one links to its anchor rather than re-narrating it. Every number below
> was independently re-verified against the repo (ROADMAP, migration headers, memory records, and
> the captured-session forensics) during the build of this playbook.

---

<a id="cs-devdb-wipe"></a>
## CS-1 — The dev-database wipe (2026-04-24)

**What happened.** An integration test (`api_db_fallback`) executed an unscoped
`DELETE FROM claude_proxy.sessions` — no `WHERE` clause. It ran against `MIKE_TEST_ADMIN_URL`,
which at the time pointed at the **same Postgres cluster** as the developer's `database_url` in
the bootstrap config. The delete took every real session row with it.

**Root cause.** Tests shared a database with development. The test's idea of "reset state" was a
table-wide delete, and nothing structurally prevented the test connection from resolving to the dev
database. A `Mutex<()>` had been used to serialize DB-backed tests — but serialization only protects
against *intra-suite* concurrency; it did nothing to stop a single test from deleting shared data.

**Blast radius.** All persisted sessions gone; persistence had to be hard-reset.

**Fix (shipped).** Isolation by construction: `TestDb::bootstrap` now *refuses* any admin URL whose
database component is `claude_proxy` (the dev DB name); every DB-backed test provisions its **own
ephemeral database** via `CREATE DATABASE claude_proxy_test_<uuid> TEMPLATE claude_proxy_test_template`
and tears it down explicitly. The serializing mutex was deleted. `#[ignore]` was rejected as a
"safety" mechanism — CI, agents, and `-- --ignored` all still fire ignored tests.

**Transferable rule.** Make it *structurally impossible* for a test to touch the dev/prod database;
give every DB-backed test its own ephemeral DB cloned from a migrated template; never reach for a
mutex to paper over missing isolation; ban unscoped `DELETE` in tests.

**Grounds:** Part IV ch. 11 (DB-backed test isolation). Referenced by ch. 3, 6, 7.

---

<a id="cs-sessionid-dataloss"></a>
## CS-2 — The session-id NOT-NULL data loss (2026-05-26)

**What happened.** A schema change added a `NOT NULL` constraint involving `session_id` and updated
the request-insert path. But there were **two** INSERT sites for the same table, sitting roughly
**1,711 lines apart** in the source. Only one was updated. The other began failing (or writing rows
that violated the new contract) silently, and the gap was not noticed for ~50 hours of traffic.

**Root cause.** Scope was declared before discovery. The developer found write sites by following
struct/function references — but raw SQL `INSERT` statements are string literals that
`findReferences` cannot see. The second site was invisible to LSP navigation.

**Blast radius.** ~50 hours of degraded/dropped capture before detection.

**Fix (shipped discipline).** Before any schema change, grep **all** write sites by *table name* in
the raw SQL (`Grep "INSERT INTO claude_proxy.<table>"`), read **every** hit, and only then declare
the file allowlist. LSP find-references and text/AST search are complementary; neither alone is
sufficient.

**Transferable rule.** Discovery precedes scope. For any additive-`NOT NULL` or signature change,
enumerate every write site by searching the SQL text, not just the type graph — and read all hits,
not just the first.

**Grounds:** Part II ch. 5 (migrations: discovery-before-scope). Referenced by ch. 6, 12, 16, 17.

---

<a id="cs-embeddings-oom"></a>
## CS-3 — The in-process embeddings OOM (2026-05-30)

**What happened.** Flipping `[embedding] enabled = true` drove the `claude-proxy` process RSS to
**~18 GB** (the operator killed it). Embeddings ran **inside the proxy process** via `fastembed` →
ONNX Runtime, driven from the **request hot path**: a write-path "embed nudge" fired on every
primary-turn request, fanning out up to **12-wide** (`embedding_permits = available_parallelism()`).
When the flag flipped on, a cold backlog of **35,564 requests / 258 sessions** became eligible at
once. RSS pinned **flat at exactly 9.29 GB for 4+ minutes**; the memory-pressure cap manager logged
`bytes_freed=0`, `admission_throttled=true`, and began **dropping live request capture**
(`forwarding without capture`) trying to relieve pressure it physically could not touch.

**Root cause.** `spawn_blocking` relieves the async runtime but **does not isolate memory** — the
ONNX C++ arena lives in the same process heap, and `fastembed` 4.9.1 exposes **no arena-release
knob**, so the only reclaim is dropping the model or killing the process. The cap manager accounts
and sheds only the in-memory `RequestStore`; native C++ memory is invisible to it. A flat ceiling
(not a rising curve) is the signature of a bounded-but-huge concurrent working set.

**Blast radius.** Proxy OOM-killed; live capture degraded before the kill.

**Fix (decided, NOT shipped).** Embeddings reverted to **OFF**. The decided direction — validated by
the incident, implementation pending — is a **standalone out-of-process worker** that ticks the
existing DB queue (`closed_at IS NOT NULL AND embedded_at IS NULL ... FOR UPDATE SKIP LOCKED`),
single-flight, OS-memory-capped (`ulimit -v`/cgroup), and independently restartable — if it bloats,
killing it never touches the proxy. The proxy keeps writing queue rows and keeps the pg_trgm FTS
recall fallback. The in-proxy embedding drivers are slated for **removal** at that cutover.

**Transferable rule.** Heavy native/ML work (ONNX, codecs) belongs in a separate, OS-capped,
restartable process — never on the request hot path, and never trusted to `spawn_blocking` for
memory safety. A memory controller can only shed what it accounts for; keep the unsheddable allocator
out of the process entirely.

**Grounds:** Part III ch. 8 (memory & resource safety). Referenced by ch. 2 (hot path), ch. 10
(cap-manager blind spot), ch. 3 (workload placement).

---

<a id="cs-mvcc-cascade"></a>
## CS-4 — The MVCC cascade: one lock holder, three symptoms

**What happened.** Several different operations on the `sessions` table all went slow at once.
Reactively optimizing each query did nothing. The real cause was a single lock holder: a
`sessions` UPDATE executed **inside the wide `snapshot_request` transaction** held a row lock long
enough that a concurrent `DELETE` (the TTL sweep) blocked behind it for ~30 seconds, and that stall
radiated to every other operation queued on the same rows.

**Root cause.** A transaction co-located writes that did **not** need to be atomic together; the
best-effort `sessions` UPDATE rode inside the same tx as the load-bearing request snapshot, widening
the lock window. `EXPLAIN ANALYZE` of the slow statement run **uncontended** completed in ~1.117 s
versus ~30.098 s in production — proof that the production cost was **wait, not work**, so tuning the
query itself was wasted effort.

**Blast radius.** ~30 s stalls cascading across `sessions` operations under concurrency.

**Fix.** Keep transactions narrow — co-locate only writes that must be atomic; move the best-effort
`sessions` UPDATE outside the snapshot tx. (Prescribed; the playbook does not assert the current code
already does this — verify against live `snapshot_request` before claiming it.) Ops backstop:
`idle_in_transaction_session_timeout` / `lock_timeout` would have bounded the blast radius.

**Transferable rule.** When many operations on one table slow down together, find the single lock
holder before optimizing any query; an uncontended `EXPLAIN` that runs fast proves the slowness is
contention. Keep transactions as narrow as atomicity actually requires, and set Postgres
statement/lock/idle-in-transaction timeouts as a backstop.

**Grounds:** diagnostic method — Part III ch. 9 (error handling). Proof technique — Part IV ch. 10
(observability). Fix — Part II ch. 6 (persistence boundary). Ops backstop — Part IV ch. 15 (ops).

---

<a id="cs-agent-forensics"></a>
## CS-5 — The agent-forensics review (2026-04 / 2026-05)

**What happened.** Because the proxy captures the *development* traffic with byte-identity, the team
could run forensics on the AI agents that were building the proxy itself: **24 sessions / 5,358
captured requests**. The review surfaced repeated process failures with hard evidence:

- A load-bearing routing rule sat **~9,000 characters into a 33,859-character** instruction block; the
  agent ignored it on first read and only honoured it after three escalations — *instruction placement
  and surface area determine adherence, independent of how emphatic the wording is*.
- Mandatory `MUST`/`ALWAYS` routing language was tested and **failed** — stronger language produced
  more confident *overriding*, not more delegation. Prose is a probabilistic lever for a structural
  problem.
- A filter rule's blast radius, seen at the byte level: the client sent **30,628 bytes**, the upstream
  received **138** — a 222× reduction the operator could see only because capture was full-fidelity.
- The irony that crystallized the no-truncation rule: a `line-clamp` was added to a UI surface *while
  the assigned task was to remove truncation*.
- The recurring meta-finding: diagnoses were produced but fixes were not shipped — a diagnosis with no
  committed fix is worthless.

**Transferable rule.** Instrument the development process itself, at full fidelity, so you can do
forensics on agent behaviour; place load-bearing rules where they win (early, short, strongest
channel); enforce routing structurally, not with stronger prose; and treat "completion" as a pasted
artifact, not a sentence.

**Grounds:** Part V ch. 16–17 (building with AI agents). The capture substrate that made it possible —
Part IV ch. 10 (observability, "capture the dev process itself").
