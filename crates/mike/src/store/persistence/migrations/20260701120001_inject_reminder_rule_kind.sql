-- inject_reminder rule kind + Claude-in-Chrome lazy_section normalization.
--
-- Part of a multi-agent feature (the Rust side is wired in parallel). Two
-- independent, expand-only, idempotent changes:
--
-- 1. New rule kind `inject_reminder` (id 4).
--    `inject_reminder` is a steering kind that, on a trigger, injects an
--    operator-authored reminder into the prompt as a <system-reminder>
--    (sibling of `lazy_section`, which strips-then-reinjects a heading span).
--    As with every other kind, there is NO stringly-typed `kind` column on
--    rules.rules and therefore no CHECK to extend: kind is normalised as
--    `kind_id bigint NOT NULL REFERENCES rules.kinds(id)` (lookup table, see
--    20260604000001_baseline.sql). Admitting the kind == inserting its row.
--    Explicit id 4 continues the deterministic seeding (1=text_substring,
--    2=xml_tag, 3=lazy_section). The kind code string is fixed as
--    'inject_reminder' and MUST NOT be renamed (Rust side keys on it).
--
-- 2. Normalize the Claude-in-Chrome lazy_section rule (UPDATE only).
--    The operator hand-seeded a lazy_section rule whose value is the
--    '# Claude in Chrome browser automation' heading. We bring it in line
--    with the new on-demand-reminder behaviour: trigger_token '<chrome>'
--    (so the marker-derived wrapper emits <chrome>...</chrome>) and a LEAN
--    stub body (the full browser-automation guidance loads on demand via
--    the claude-in-chrome MCP tool). This is an UPDATE scoped to the
--    existing heading — a no-op on a DB that has no such rule.
--
--    Deliberately NOT an INSERT/seed: seeding an actual rules.rules row (and
--    its overrides.overrides composition side-row) into every fresh /
--    template DB pollutes the per-test ephemeral databases that the
--    integration suites clone from, breaking suites that assert a pristine
--    baseline (composition list counts, rule list emptiness). The operator's
--    real need — their persistent DB, every current and future session — is
--    met by the UPDATE-normalize above; their rule already exists. A brand-
--    new deployment creates the Chrome rule via the filter workbench (as the
--    original was), so we keep the migration set free of standing app-data
--    seeds. (Contrast 20260616000004_steer_author_seed, which DOES seed a
--    composition into the template; we intentionally do not add another.)
--
-- Performance: pure DDL + one scoped UPDATE; nothing on a hot path, no scan
-- of a large relation. Perf-Exempt: one-shot DDL + a constant-bounded
-- UPDATE scoped by a literal predicate, cannot regress any query path.

-- ── 1. New rule kind ────────────────────────────────────────────────────
INSERT INTO rules.kinds (id, code) OVERRIDING SYSTEM VALUE
VALUES (4, 'inject_reminder')
ON CONFLICT DO NOTHING;

-- Re-sync the identity sequence past the highest explicitly-assigned id so a
-- future plain INSERT into rules.kinds does not collide (the baseline seeded
-- ids with OVERRIDING SYSTEM VALUE without advancing the sequence). Idempotent.
SELECT setval(
    pg_get_serial_sequence('rules.kinds', 'id'),
    (SELECT max(id) FROM rules.kinds),
    true
);

-- ── 2. Normalize the existing Chrome lazy_section rule (no-op if absent) ──
UPDATE rules.rules
SET trigger_token = '<chrome>',
    text = E'# Claude in Chrome browser automation\n\nBrowser-automation guidance loads on demand. Before your first browser action, output `<chrome>` in your text and call `mcp__claude-in-chrome__tabs_context_mcp` — the full guidance arrives as a `<chrome>` reminder alongside the tool result. Don''t use other browser tools before it loads.',
    updated_at = NOW()
WHERE kind_id = (SELECT id FROM rules.kinds WHERE code = 'lazy_section')
  AND value = '# Claude in Chrome browser automation';
