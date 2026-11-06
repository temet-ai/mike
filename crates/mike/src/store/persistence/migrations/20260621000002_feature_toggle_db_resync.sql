-- Phase 4 / Local-DB Fallback Resync: make `db_resync` a runtime feature toggle.
--
-- The resync worker (crates/claude-proxy/src/resync/worker.rs) gates
-- post-DB-connection background work — it merges local-only capture rows back
-- into the remote DB once it returns. Unlike the connection-bootstrap config
-- (`database_url` / `local_database_url` / `db_fallback_enabled`, which are read
-- BEFORE the DB connects and so must stay env/CLI), the resync worker runs after
-- both pools exist and so belongs in the runtime-mutable feature-toggle system
-- alongside `lesson_worker` / `steerer` (.claude/rules/no-default-off.md).
--
-- `config.feature_toggles.key` carries a closed CHECK constraint
-- (20260615000003_feature_toggles.sql) enumerating the allowed keys, so adding a
-- seventh toggle requires:
--   1. dropping and re-adding the CHECK with the new key included, and
--   2. seeding the new row enabled=true (no-default-off rule).
--
-- DDL only — no application queries ship here. Idempotent: the constraint is
-- dropped IF EXISTS before re-creation, and the seed is ON CONFLICT DO NOTHING.

ALTER TABLE config.feature_toggles
    DROP CONSTRAINT IF EXISTS feature_toggles_key_check;

ALTER TABLE config.feature_toggles
    ADD CONSTRAINT feature_toggles_key_check
    CHECK (key IN (
        'reminders',
        'recall_injector',
        'weasel_intercept',
        'steerer',
        'lesson_worker',
        'harvester',
        'db_resync'
    ));

-- Seed the new toggle enabled=true (no-default-off rule). Idempotent.
INSERT INTO config.feature_toggles (key, enabled) VALUES
    ('db_resync', true)
ON CONFLICT (key) DO NOTHING;
