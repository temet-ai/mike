# Omit integration query measurements

Measured on 2026-10-17 with `scripts/db-perf-check`, using the configured live
capture database, 25 runs per query, read tier (50 ms budget). These are read-only
PostgreSQL catalog and migration-ledger queries; no fixture data or captures were
written. New queries have baselines; the changed ledger projection and relocated
schema probe have before/after measurements.

```text
Perf-Tier: read
Perf-Label: omit_install_assessment
Perf-After-ms: 1.193
Perf-Budget-ms: 50
Perf-Verdict: baseline
Perf-Query-SHA: c58b6bfcfd93b486
```

```text
Perf-Tier: read
Perf-Label: omit_failed_ledger
Perf-After-ms: 0.077
Perf-Budget-ms: 50
Perf-Verdict: baseline
Perf-Query-SHA: 1fa050c16b31f4a0
```

```text
Perf-Tier: read
Perf-Label: omit_install_extensions
Perf-After-ms: 2.158
Perf-Budget-ms: 50
Perf-Verdict: baseline
Perf-Query-SHA: 122382b5a1b53431
```

```text
Perf-Tier: read
Perf-Label: omit_migration_ledger
Perf-Before-ms: 0.103
Perf-After-ms: 0.109
Perf-Budget-ms: 50
Perf-Verdict: neutral
Perf-Query-SHA: b6f833c4d82fa871
```

```text
Perf-Tier: read
Perf-Label: omit_schema_probe
Perf-Before-ms: 0.077
Perf-After-ms: 0.074
Perf-Budget-ms: 50
Perf-Verdict: neutral
Perf-Query-SHA: 52740efd846e7332
```
