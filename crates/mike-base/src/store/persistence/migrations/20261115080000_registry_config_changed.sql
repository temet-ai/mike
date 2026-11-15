-- Steerers and filter rules are switches too: turning a steerer or a rule on
-- or off on one machine must reach every running mike without a restart
-- (#255). Each table gets the same statement trigger as the config tables,
-- so a write sends one NOTIFY on `mike.config_changed` and db_listener
-- reloads the steerer registry and the filter rules.

DROP TRIGGER IF EXISTS trg_steerers_config_changed ON steering.steerers;
CREATE TRIGGER trg_steerers_config_changed
AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON steering.steerers
FOR EACH STATEMENT EXECUTE FUNCTION config.notify_config_changed();

DROP TRIGGER IF EXISTS trg_filter_rules_config_changed ON rules.rules;
CREATE TRIGGER trg_filter_rules_config_changed
AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON rules.rules
FOR EACH STATEMENT EXECUTE FUNCTION config.notify_config_changed();
