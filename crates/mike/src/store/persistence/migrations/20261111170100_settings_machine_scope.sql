-- Issue temet-ai/mike#84: a setting that describes a machine is stored once per machine.
--
-- config.settings keeps one row per key and stays the shared value every machine reads.
-- A machine-scoped setting (SettingScope::Machine in crates/mike/src/settings/) may also
-- hold a row here for one machine_uuid (machine_identity.rs, machine.json); a machine's
-- own row wins over the shared one when it reads its settings, and a machine with no row
-- reads the shared value. Existing config.settings rows are left as they are, so on
-- upgrade they are the shared values and no machine changes behaviour.
--
-- A separate table rather than a machine_uuid column on config.settings: every existing
-- writer of config.settings upserts with ON CONFLICT (key), which a (key, machine_uuid)
-- key would break. machine_uuid has no foreign key, as in llm.worker_leases: a machine
-- writes its own value before it has registered any project location.
--
-- Every change is written to config.change_log as a 'setting' row whose subject_key is
-- '<key>@<machine_uuid>', so the timeline names the machine and a shared key's history
-- is not mixed with a machine's.

SET LOCAL lock_timeout = '5s';

CREATE TABLE IF NOT EXISTS config.machine_settings (
    key          text        NOT NULL,
    machine_uuid uuid        NOT NULL,
    value        jsonb       NOT NULL,
    updated_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (machine_uuid, key)
);

COMMENT ON TABLE config.machine_settings IS
    'One machine''s own value of a machine-scoped setting (issue #84). Wins over config.settings for that machine; config.settings is the shared default.';

CREATE OR REPLACE FUNCTION config.log_machine_setting_change() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    k      text  := CASE WHEN TG_OP = 'DELETE' THEN OLD.key ELSE NEW.key END;
    m      uuid  := CASE WHEN TG_OP = 'DELETE' THEN OLD.machine_uuid ELSE NEW.machine_uuid END;
    old_v  jsonb := CASE WHEN TG_OP = 'INSERT' THEN NULL ELSE OLD.value END;
    new_v  jsonb := CASE WHEN TG_OP = 'DELETE' THEN NULL ELSE NEW.value END;
    secret boolean := config.is_secret_setting_key(k);
BEGIN
    IF TG_OP = 'UPDATE' AND old_v IS NOT DISTINCT FROM new_v THEN
        RETURN NULL;
    END IF;
    PERFORM config.append_change(
        'setting', k || '@' || m::text, lower(TG_OP),
        CASE WHEN secret AND old_v IS NOT NULL THEN '{"redacted": true}'::jsonb ELSE old_v END,
        CASE WHEN secret AND new_v IS NOT NULL THEN '{"redacted": true}'::jsonb ELSE new_v END,
        secret);
    RETURN NULL;
END $$;

DROP TRIGGER IF EXISTS trg_machine_settings_change_log ON config.machine_settings;
CREATE TRIGGER trg_machine_settings_change_log
    AFTER INSERT OR UPDATE OR DELETE ON config.machine_settings
    FOR EACH ROW EXECUTE FUNCTION config.log_machine_setting_change();
