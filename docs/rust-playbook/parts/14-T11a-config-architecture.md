**Part IV · Threads: source-of-truth · bound-it · evidence-over-attestation · deterministic-enforcement · Grounded by: [CS-3](../case-studies.md#cs-embeddings-oom)**

# 14. Configuration architecture

Configuration is where "what an operator wrote" and "what actually runs" quietly drift apart. A value resolved through five hand-rolled `if let` merges, a cap constructed with `::default()` instead of read from the file, a script that re-greps the TOML the binary already parsed, an advice string truncated behind a logged warning — each is locally reasonable, and each produces the same failure: the live system behaves differently from the config someone authored, and nobody can say why. Treat config the way a fidelity-obsessed service treats its data path: one authoritative resolver, everything else a rebuildable projection of it, every safety knob proven effective at startup, nothing silently mutated on the way in. These are day-one choices — cheap to make, expensive to retrofit once a dozen modules each read their own env var.

---

## Lesson 1: Resolve config through a layered provider with library-named precedence — not hand-rolled merges
`[thread: source-of-truth]` `[thread: deterministic-enforcement]`

**Do this.** Pick a config library that defines the layering for you and run one chain: built-in defaults → config file (located via an XDG/etc lookup) → `PREFIX_*` env vars → CLI flags. The precedence is *named and documented by the library*, not emergent from the order you happened to write the merges in. A missing config file is **not** an error (boot on defaults, so a fresh clone runs with zero setup); a malformed file returns a structured error that **includes the resolved path**, so an operator never chases a "but the value is right there" ghost.

**Not this.** Scattered `std::env::var` reads across modules, stitched together by implicit `if let Some(x) = … else …` ordering, with opaque parse errors and no single place defining who wins. In that shape it is genuinely unknowable whether the env var or the file took effect.

**Why.** With manual merging, precedence is an accident of code order that changes silently when someone reorders the function. A provider makes precedence a named library property — "values for duplicate keys from a merged provider replace those from previous providers" ([figment docs](https://docs.rs/figment/latest/figment/)) — one you can point at and test. Pin every layer transition (`toml_overrides_defaults`, `env_overrides_toml`, `cli_overrides_env`, `malformed_toml_returns_error_with_path`) so a refactor that inverts precedence fails CI instead of production.

**Evidence.** (in temet-proxy: `figment`) `crates/claude-proxy/src/config/load.rs:202-238` — the `load_layered` pipeline, low→high; `crates/claude-proxy/src/config/paths.rs:51-83` — XDG lookup chain where `None` means boot-on-defaults; `Cargo.toml:20` — `figment = { version = "0.10", features = ["toml", "env"] }`; `load.rs:522-624` — the four named layer-precedence + malformed-path tests.

**Pattern.**

```rust
// Precedence IS the merge order, named by the provider — not buried in if-let chains.
let mut figment = Figment::from(Serialized::defaults(ProxyConfig::default()));
if let Some(path) = toml_path.as_ref() {
    figment = figment.merge(Toml::file(path));       // file beats defaults
}
figment = figment.merge(env_provider());              // env beats file
figment = figment.merge(Serialized::defaults(cli));   // CLI beats env
figment.extract().map_err(|e| match toml_path.as_ref() {
    Some(p) => anyhow!("failed to load config from {}: {e}", p.display()),
    None    => anyhow!("failed to load config: {e}"),
})
```

```mermaid
flowchart BT
    A["built-in defaults"] -->|wins on conflict| B["bootstrap config (XDG lookup chain)"]
    B -->|wins on conflict| C["PREFIX_* env vars"]
    C -->|wins on conflict| D["CLI flags"]
    D --> E["resolved ProxyConfig"]
    note["missing file = boot on defaults; malformed = error WITH resolved path"]
```

---

## Lesson 2: Make the binary the single source of resolved config; scripts `eval` it, never re-parse
`[thread: source-of-truth]`

**Do this.** When ops scripts need resolved values (ports, bind addresses), add a `print-config --shell` subcommand that emits `KEY=VALUE` lines from the binary's **own fully-resolved** config, and have scripts `eval` it. The binary walks the defaults→file→env→CLI chain exactly once, so the script and the running process can never disagree.

**Not this.** Grep/sed-parsing the TOML in bash (`grep -E '^database_url' | sed …`). That reads layer 2 only — it silently ignores every env-var and CLI override and drifts from what the process actually loaded.

**Why.** Two independent config readers are two sources of truth; they diverge the first time someone sets an env override the script doesn't know about. One authoritative reader, projected into the shell, keeps them structurally identical — the data-path source-of-truth rule from [ch. 10 — Observability & inspection fidelity](./10-T09-observability-fidelity.md) applied to config.

**Caveat — secrets.** A `print-config --shell` that redacts secrets (masking the DB password to `postgres://user:***@host/db` so the output is safe to paste into an issue) is good hygiene, but it means the `eval`'d secret env var is the *redacted* form. A script relying on `eval` output for an actual DB connection must source the real secret elsewhere — operational deploy scripting owns reconciling this ([ch. 15 — Operational safety & deploy](./15-T11b-ops-deploy-safety.md)).

**Evidence.** `scripts/start-proxy.sh:81-82` — `eval "$("${CLAUDE_PROXY_BIN}" print-config --shell)"` with the "no drift between two config reads" comment; `scripts/deploy-proxy.sh:181` — identical `eval` to resolve bind addrs before the smoke test; `crates/claude-proxy/src/config/load.rs:272-382` — `to_shell_exports` (deterministic shell-quoted `KEY=VALUE`); `load.rs:390-416` — `redact_database_url` (the secret-masking caveat); `crates/claude-proxy/src/config/subcommands.rs:43-48,174-182` — the `--shell` flag.

**Pattern.**

```bash
# The binary is the authoritative config reader; the script is a consumer of it.
eval "$("${CLAUDE_PROXY_BIN}" print-config --shell)"
# $MIKE_LISTEN_ADDR / $MIKE_INSPECT_ADDR now match the running process exactly.
```

```mermaid
sequenceDiagram
    participant S as start-proxy.sh
    participant B as binary (print-config --shell)
    S->>B: resolve defaults<file<env<CLI ONCE
    B-->>S: KEY=VALUE lines
    Note over S,B: script & process see identical values
    Note over S: ✗ grep the bootstrap config would read layer 2 only
```

---

## Lesson 3: Ship a self-documenting config template plus introspection subcommands
`[thread: source-of-truth]` `[thread: evidence-over-attestation]`

**Do this.** Track a bootstrap template where every key appears commented-out at its built-in default with inline rationale — the template *is* the reference docs. Pair it with `print-config` for the resolved effective values and a separate bootstrap writer that refuses to clobber an existing file without `--force`. Together they erase the "which config is live?" guess.

**Not this.** Config keys discoverable only by reading source; no way to ask the binary what the effective values are; a bootstrap writer that silently overwrites an operator's hand-edited file.

**Why.** "What's the live value?" answered by a subcommand is evidence; answered by reading source and reasoning about precedence is a guess that's wrong the moment an env override exists. The `--force` clobber-guard is what makes the convenience command safe.

**Evidence.** Bootstrap template lines 1-18 — lookup-chain + precedence header; `crates/claude-proxy/src/config/subcommands.rs:60-87` — the `STARTER_TOML` template constant; `subcommands.rs:199-210` — `write_starter_toml` refuses overwrite without `--force`; `subcommands.rs:264-281` — the overwrite-refusal test; `subcommands.rs:184-190` — the effective config source is surfaced.

---

## Lesson 4: Operator-authored config must round-trip verbatim — never mutate it behind a warning
`[thread: source-of-truth]`

**Do this.** A config value the operator wrote that is later rendered back *as itself* (advice text, a correction prompt, a label) must survive load unchanged. "What you see is what you stored" is a load-time invariant. If a value is too large for some downstream surface, fix the surface — paginate, scroll — do not cap the source.

**Not this.** A `MAX_ADVICE_CHARS = 1024` cap that truncates operator-supplied text during config loading with only a logged warning, so the rendered value silently diverges from the authored TOML. A warning in a log nobody reads is not consent.

**Why.** Once load-time mutation is acceptable for one field, it's acceptable for the next, and the config file stops being a faithful record of intent. This is data-path truncation in config's clothing ([ch. 10 — Observability & inspection fidelity](./10-T09-observability-fidelity.md)): a field rendered as itself must round-trip. The cap here was found mutating operator input, named as an anti-pattern, and deleted.

**Evidence.** `.claude/rules/inspection-fidelity.md:77` — the catalogued anti-pattern ("`MAX_ADVICE_CHARS` 1024-char cap … mutates operator-authored config behind a warning; a TOML field rendered as itself in the UI must round-trip verbatim"); `grep -rn "MAX_ADVICE" crates/` → empty (re-run live, confirmed) — the cap is gone from source.

---

## Lesson 5: A safety control wired to its own default is a disabled control — thread config end-to-end and prove the effective value
`[thread: bound-it]` `[thread: evidence-over-attestation]` `[thread: deterministic-enforcement]`

**Do this.** Any cap, ceiling, or kill-switch must be read from config and threaded all the way to the call site that enforces it: add the field to the config struct, surface it in the sample, and make the effective value observable at startup (a boot log line or `print-config`). "Enabled but defaulted-to-zero" must be impossible to ship unnoticed. A doc-comment claiming a struct "reads from `[section]`" is worthless if no such field exists on the config type — only the *wiring* counts.

**Not this.** Constructing a safety struct with `::default()` at the call site instead of reading it from config — the struct's comment promises a TOML section that the config type doesn't even have.

**Why.** ([CS-3](../case-studies.md#cs-embeddings-oom)): the embeddings OOM had a quieter failure beneath the headline. The global RSS cap manager — the thing that should have shed memory under pressure — was built with `CapManagerConfig::default()` hardcoded at the call site, giving `rss_ceiling_bytes = 0`: observation-only, never shed, the entire time. The safety net was off and nothing surfaced it, because the only "evidence" it was wired was a struct comment naming a `[cap_manager]` section that didn't exist on the config type. [ch. 9 — Error-handling architecture](./09-T08-error-handling.md) owns the cap-*blind-spot* facet — a controller sheds only what it *accounts* for; this lesson owns the config-*wiring* facet — it enforces only what it's actually *fed*. The fix landed: a real `cap_manager` field, threaded at the call site, with `rss_ceiling_bytes = 0` now an explicit *documented* "disabled" default rather than an accidental one (Lesson 7).

**Evidence.** `artifacts/proxy-memory-oom-mitigation/GAP-ANALYSIS.md` §4.1 H2 — origin of the problem ("`CapManagerConfig::default()` hardcoded in `main.rs` → `rss_ceiling_bytes=0` → cap manager effectively disabled … struct comment claims it reads from `[cap_manager]` settings but no such field exists"); `crates/claude-proxy/src/config/mod.rs:237-241,425` — the real `cap_manager` field + `Default`; `crates/claude-proxy/src/main.rs:271-273` — `spawn_cap_manager(…, state.config().cap_manager.clone(), …)`, config-driven (fix landed in live code).

---

## Lesson 6: Fail fast on misconfiguration — no silent fallback to a degraded mode
`[thread: deterministic-enforcement]`

**Do this.** When an operator opts into a subsystem, validate it eagerly at startup and error out on a missing URL or unreachable backend. Surface the failure loudly. The same applies to runtime-supplied file paths — validate a referenced cert/binary path exists at startup, not on first use after a restart.

**Not this.** Silently falling back to an in-memory mode when a requested DB is misconfigured, so the operator believes persistence is on while data evaporates; an unchecked `ca_cert_path` that surfaces only as a confusing runtime failure hours later.

**Why.** A silent degraded mode converts a config typo into a data-loss incident discovered under load. An eager error converts it into a failed boot the operator fixes in seconds. Pin *both* failure modes (missing value, unreachable backend) with tests so the loud-failure contract can't regress. This is the boot-time face of the deploy-time preflight discipline in [ch. 15 — Operational safety & deploy](./15-T11b-ops-deploy-safety.md): catch the bad config before it can serve traffic, not after.

**Evidence.** `crates/claude-proxy/src/config/mod.rs:1118-1130` — `build_persist_handle` returns an explicit `MissingDatabaseUrl` error (not `Ok(None)`) then `connect().await? + preflight().await?`, with no silent in-memory fallback; `mod.rs:1235-1270` — `app_state_new_errors_without_database_url_when_enabled` / `app_state_new_errors_on_unreachable_database` pin both modes; corroborating: `artifacts/research/proxyclawd-audit.md:76-81` (unvalidated env/paths → silent failure, the shape to avoid), `artifacts/research/rust-developer-guide.md:734-746` (validate all config at startup).

---

## Lesson 7: Default *optional* subsystems off so a bare run boots — but make the product's core mandatory, and own the cost
`[thread: bound-it]`

**Do this.** Expensive *optional* features (an RSS cap manager, a request filter, response-buffering interceptors, destructive startup sweeps) should default **off** so a fresh clone with no config boots on defaults with zero infrastructure, each opt-in via its own TOML sub-table. Combined with Lesson 6, this yields loud failures *only* when the operator actually asked for the feature. A cap whose default disables it must be a **deliberate, documented** default (Lesson 5's `rss_ceiling_bytes = 0 # disabled`), never an accidental one.

**Not this.** A flat "everything defaults off" rule applied blindly. The honest position is per-subsystem: features default off **unless they are core to the product** — in which case making them mandatory (and accepting that a bare run now requires that dependency) is the correct, deliberate trade, documented as the exception with its cost stated.

**Why.** Persistence is the deliberate counterexample, stated honestly rather than pretending the rule is universal. For a service whose entire value is captured traffic, persistence is mandatory: `persist_enabled` defaults **true** while `database_url` defaults `None`, so a bare `cargo run` *fails fast* on the missing URL (Lesson-6 test) — the opposite of "bare run boots," and the right call here. A side-lesson: the stale `ARCHITECTURE.md` paragraph still claiming `persist_enabled` defaults *false* is a live "changing a feature spans docs, not just source" — trust the code, fix the doc.

**Evidence (genuinely-off subsystems).** `crates/claude-proxy/src/config/mod.rs:125-126` filter off; `mod.rs:142-143` weasel_intercept off; `mod.rs:237-241` cap_manager `rss_ceiling_bytes = 0` disabled; `mod.rs:412` `sweep_at_startup: false`. **Persistence exception:** `mod.rs:402-407` (`persist_enabled: true`, `database_url: None`); gap-analysis L3 "mandatory persistence." **Do NOT cite as off** (they default ON): `reminders`, `lesson_worker` (`mod.rs:347`), `composition_enabled` (`mod.rs:417`).

---

## Lesson 8: Give every unbounded buffer a configurable cap with an explicit, honest backpressure response
`[thread: bound-it]`

**Do this.** Any in-memory buffer fed by external input — concurrent in-flight requests, inbound/outbound body sizes, the capture store — is a DoS/OOM vector *even on loopback*. Give each a configurable cap with a sane default; respond to the cap with an explicit honest signal (HTTP 503 + `Retry-After` for an in-flight registry, 413 for an oversize request body, 502 with `{"truncated": false}` rather than a silently sliced response); and let `0` mean "deliberately disable this cap" rather than leaving it implicitly unbounded.

**Not this.** Pushing arbitrary external bytes into a `Vec`/registry until OOM; truncating a response silently to fit instead of refusing it (which violates a byte-identity contract); hard-coding a cap with no override and no documented escape.

**Why.** A cap that returns 502-`truncated:false` over a partial body is the config-layer expression of the data-path no-slicing rule ([ch. 10 — Observability & inspection fidelity](./10-T09-observability-fidelity.md)) — refusal over silent corruption. This is the *config facet* of bounding; the memory-accounting facet (what the store costs, how it sheds) lives in [ch. 8 — Memory & resource safety](./08-T06-memory-resource-safety.md), and channel/stream backpressure mechanics in [ch. 7 — Async & concurrency discipline](./07-T07-async-concurrency.md). Same thread, three distinct surfaces — wire all three.

**Evidence.** `crates/claude-proxy/src/config/mod.rs:167-217` — `max_request_body_bytes` (413), `max_response_body_bytes` (502 + `truncated:false`), `max_in_flight` (503 + `Retry-After`), `max_store_bytes` (LRU); bootstrap template lines 25-68 — each cap documented with `Set to 0 to disable`; `mod.rs:1044-1052` — `max_in_flight` threaded into `InFlightRegistry::with_capacity`.

---

## Lesson 9: When a value has multiple representations, name one the source of truth and make the rest rebuildable projections
`[thread: source-of-truth]`

**Do this.** With several live representations of the same state (in-memory `ArcSwap`, DB, on-disk config file), pick the authoritative ones and treat the rest as projections. On a write: update the sources of truth first (in-memory + DB), then write the file projection; if the file write fails, log/surface the error but do **not** roll back the sources — the next successful write heals the divergence. Coordinate the file-watcher and the API writer through one shared single-writer lock carrying a post-write `mtime`, so an API write doesn't trigger the reload its own write caused.

**Not this.** Treating the config file as a co-equal source of truth and rolling back DB writes when the file write fails; an uncoordinated watcher + API writer causing write→reload→rewrite loops; "fixing" that loop with debounce or content-hash compare (both still parse every tick and still race fast writes).

**Why.** Three writers to three representations with no designated authority deadlock or loop. Naming the sources of truth and making the file a projection means a failed file write is a recoverable lag, not a corrupted state — and one `mtime`-carrying lock lets the watcher skip the reload it triggered itself. ArcSwap's lock-free *read* mechanics belong to [ch. 7 — Async & concurrency discipline](./07-T07-async-concurrency.md); this lesson owns the reconcile-across-representations facet.

**Evidence.** (in temet-proxy: the filter-rule subsystem — TOML `[filter]` + DB + in-memory `ArcSwap`) `crates/claude-proxy/src/config/mod.rs:485-508` — `FilterWriteGuard` doc + `last_mtime` (three-surface single-writer + mtime skip); `mod.rs:536-552` — `filter_config: ArcSwap`, `filter_write_lock`, `filter_service`; `mod.rs:944-968` — `reconcile_on_startup` wiring; corroborating: `artifacts/reviews/phase3-design-2026-04-23.md` (design review behind it, mtime reload-loop prevention).

```mermaid
flowchart TD
    W["write: rule mutation"] --> M["update in-memory ArcSwap (source of truth)"]
    W --> DB["update DB (source of truth)"]
    M --> F["write bootstrap config (projection)"]
    DB --> F
    F -. "file write fails" .-> L["log/surface — do NOT roll back sources"]
    L -. "next successful write heals" .-> F
    G["single FilterWriteGuard + last_mtime"] -. serializes .- M
    G -. "watcher skips self-triggered reload" .- F
```

---

## Lesson 10: Inject config for behaviour tests; reserve env-mutation for tests of the env layer itself
`[thread: deterministic-enforcement]`

**Do this.** For tests asserting behaviour *under a given config*, pass the config struct in through a helper (`start_proxy_with_analysis(uri, analysis)`) rather than setting process env vars. Env mutation is process-global; the config provider reads it at extract time, so two parallel env-mutating tests see each other's vars and you're forced into a crate-wide serial mutex. When you genuinely must test the **env-loading layer**, isolate it behind a serial lock plus a restore-on-drop guard — that is the one legitimate place to mutate env.

**Not this.** Routinely setting/removing process env to configure the system under test, dragging in `serial_test`/`env_lock` and producing order-dependent flakes — when the test only cares about behaviour under config X, not about how X is loaded from env.

**Why.** A flat "never mutate env in tests" would contradict the loader these lessons are drawn from — its own tests *must* touch env, because the env layer is the thing under test. The narrower rule: inject the struct for behaviour, mutate env only when env-loading is the assertion target. The migration that proved it dropped seven `serial_test`/`remove_var` pairs precisely because no env mutation happened anywhere else, so cross-test serialisation was no longer required.

**Evidence.** commit `5bd0d52` — migrated two test binaries off the legacy `MIKE_CONFIG` env-var seam; `start_proxy_with_analysis(uri, analysis)` injects an `AnalysisConfig` in-process; dropped seven `serial_test`/`remove_var` pairs (verified via `git show 5bd0d52`); `crates/claude-proxy/src/config/load.rs:443-500` — `env_lock()` + `EnvVarGuard` restore-on-drop, the legitimate env-layer-test exception; `load.rs:627-666` — `every_documented_env_mapping_reaches_the_right_field` (must touch env; it tests the env layer).

---

## Further reading

- [figment — merge-order precedence](https://docs.rs/figment/latest/figment/) · Tier 1 · REINFORCES Lesson 1 — confirms precedence is a library property ("values for duplicate keys from a merged provider replace those from previous providers"), grounding "precedence is a property you can point at and test, not an accident of merge order."
- [The Twelve-Factor App — III. Config](https://12factor.net/config) · Tier 1 · REINFORCES Lessons 2 & 9, QUALIFIES Lessons 1 & 7 — the "one place owns config" principle and its no-credentials-in-code litmus underwrite the single-resolved-source rule and the secret-redaction caveat. But it *diverges* on shape: 12factor argues against grouping config into named environments and for flat orthogonal env vars, whereas this chapter deliberately uses a structured, file-first config (TOML sub-tables) with env as one override layer. The divergence is principled — 12factor optimizes for cloud deploy-per-config; a local single-binary proxy optimizes for a self-documenting authoritative file (Lesson 3). Name the tension; don't claim 12factor endorses the design.
