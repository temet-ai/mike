-- Remove the filter-rule composition stubs that
-- `seed_filter_composition` (crates/mike/src/store/persistence/filter_rules.rs,
-- removed in this same change) used to write. Their stated purpose was suppressing
-- filter-induced anomalies via a `source='filter'` predicate in the
-- anomalies query; that predicate is gone (the real bug it worked around
-- was `filter_modified` flagging any body-rewriting stage, not just a
-- fired filter rule -- fixed by keying the anomaly query on
-- `firing_rule_ids` instead). With no remaining reader, the stubs are
-- dead weight: `prompt_sha256 = sha256('filter:' || rule.uuid)` occupies a
-- key space the composer's `sha256(system[2].text)` lookup can never
-- produce, so they were always structurally unmatchable to real traffic
-- (findings-brief.md Defect 2), reporting `count_total: 0` forever.
--
-- Sequenced after the anomaly-query fix so there's no window where that
-- query still reads a `source='filter'` row this migration is about to
-- remove.
--
-- Scope, verified live before writing this migration: 33 `source='filter'`
-- rows (one per filter rule, auto-seeded on create) plus 1
-- 'SPSA-VERIFY-step3-TEMP-DELETE-ME' manual-test-litter row. Zero
-- `requests.system_prompts.override_id` rows reference either set (the
-- disjoint sha space means no real request could ever have been attributed
-- to one), so the FK on that column never blocks this delete;
-- `overrides.revisions.override_id` is `ON DELETE CASCADE` and carries no
-- rows against this set either.
--
-- The final DELETE is scoped to the exact shas just removed above (via
-- RETURNING, not a blanket "any orphan" anti-join) -- a live probe found
-- 158 pre-existing orphaned system_prompts rows unrelated to these stubs
-- (no overrides row, no request observation, for reasons this migration
-- has no mandate to investigate); a table-wide anti-join would have swept
-- those up too. Scoping to the just-deleted shas keeps this a precise
-- cleanup, not a general orphan sweep.
--
-- Idempotent: every DELETE is a no-op once its target rows are gone; a
-- second run against an already-cleaned DB deletes zero rows (the CTEs
-- RETURN nothing, so the final DELETE's driving set is empty).
--
-- Perf-Exempt: one-shot DELETE over ~34 rows plus their now-orphaned
-- system_prompts siblings, scoped by an indexed source_id join and a
-- literal label match -- no full-table scan, migration-tier budget (5s)
-- is unreachable to approach at this row count.

WITH removed_filter_stubs AS (
    DELETE FROM overrides.overrides ov
        USING overrides.sources src
        WHERE src.id = ov.source_id AND src.code = 'filter'
    RETURNING ov.id, ov.prompt_sha256
), removed_test_litter AS (
    DELETE FROM overrides.overrides
        WHERE label = 'SPSA-VERIFY-step3-TEMP-DELETE-ME'
    RETURNING id, prompt_sha256
), removed AS (
    SELECT id, prompt_sha256 FROM removed_filter_stubs
    UNION ALL
    SELECT id, prompt_sha256 FROM removed_test_litter
)
-- The `ov.id NOT IN (SELECT id FROM removed)` exclusion (not a bare
-- `NOT EXISTS` against overrides.overrides) is load-bearing: sibling
-- writable CTEs in one WITH clause don't see each other's deletes, so a
-- subquery reading overrides.overrides directly still sees the
-- soon-to-be-gone stub row and would wrongly conclude every sha stays
-- referenced. Excluding by the ids this same statement already removed
-- sidesteps that visibility rule instead of fighting it.
DELETE FROM system_prompts.system_prompts sp
USING (SELECT DISTINCT prompt_sha256 FROM removed) rs
WHERE sp.prompt_sha256 = rs.prompt_sha256
  AND NOT EXISTS (
        SELECT 1 FROM overrides.overrides ov
        WHERE ov.prompt_sha256 = sp.prompt_sha256
          AND ov.id NOT IN (SELECT id FROM removed)
      )
  AND NOT EXISTS (SELECT 1 FROM requests.system_prompts rsp WHERE rsp.prompt_sha256 = sp.prompt_sha256);
