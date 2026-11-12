-- config.feature_toggles.key loses its closed CHECK constraint (ADR 0003, D4).
--
-- Every new toggle used to need a migration that dropped and re-added
-- feature_toggles_key_check with one more key: 65 of 354 migrations did only that.
-- The key space is already closed in code: FeatureToggleService::set_with_source
-- refuses any key not in TOGGLE_META before it writes, and the service is the only
-- production writer. A row whose key the code does not know is skipped when the
-- overlay is loaded.
ALTER TABLE config.feature_toggles
    DROP CONSTRAINT IF EXISTS feature_toggles_key_check;
