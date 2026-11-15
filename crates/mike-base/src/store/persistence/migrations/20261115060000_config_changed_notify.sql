-- Every running mike reloads its feature toggles and settings when another
-- machine (or another process on this one) writes them. A statement-level
-- trigger on each config table sends one NOTIFY on channel
-- `mike.config_changed` with the table name as payload; crates/mike's
-- db_listener LISTENs on it and rebuilds both overlays (#255).

CREATE OR REPLACE FUNCTION config.notify_config_changed()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('mike.config_changed', TG_TABLE_NAME);
    RETURN NULL;
END; $$;

DROP TRIGGER IF EXISTS trg_feature_toggles_config_changed ON config.feature_toggles;
CREATE TRIGGER trg_feature_toggles_config_changed
AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON config.feature_toggles
FOR EACH STATEMENT EXECUTE FUNCTION config.notify_config_changed();

DROP TRIGGER IF EXISTS trg_settings_config_changed ON config.settings;
CREATE TRIGGER trg_settings_config_changed
AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON config.settings
FOR EACH STATEMENT EXECUTE FUNCTION config.notify_config_changed();

DROP TRIGGER IF EXISTS trg_machine_settings_config_changed ON config.machine_settings;
CREATE TRIGGER trg_machine_settings_config_changed
AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON config.machine_settings
FOR EACH STATEMENT EXECUTE FUNCTION config.notify_config_changed();
