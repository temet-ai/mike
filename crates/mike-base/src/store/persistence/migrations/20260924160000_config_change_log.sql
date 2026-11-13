-- JC05 (judgement-context program): one timeline of configuration changes.
--
-- Why: config.settings and config.feature_toggles are overwritten in place
-- (store/persistence/settings.rs:15-36, store/persistence/feature_toggles.rs:20-42),
-- and steering.steerers rows are updated in place (store/persistence/steering/fleet.rs:195-259).
-- On 2026-08-31 the turn judge's model, rubric and input changed within 17 minutes;
-- stored `corrected` recall on real operator turns fell from 24.8% to 6.8% and nobody
-- could later establish who changed judge_worker.model or when (fleet-mirror
-- analysis.md, issue-drafts.md Draft 9). This table records every change from any
-- writer: mike, the CLI, a migration or a psql session.
--
-- Mechanism: AFTER ROW triggers on the three tables. Mike's writers attach provenance
-- through the transaction-local setting mike.change_source (a JSON object with via,
-- actor, request_id, build_sha, detail). When it is absent the surface is derived
-- from application_name. Secret-shaped setting keys are redacted by a predicate that
-- mirrors settings/service.rs:22-30. No-op updates write nothing.
--
-- Expand-only: new schema objects plus one baseline INSERT per existing row. Nothing
-- existing is altered or dropped.
--
-- CREATE TRIGGER takes SHARE ROW EXCLUSIVE on the three tables, which blocks only
-- their (rare, operator-driven) writes. A 5 s lock_timeout bounds the wait so a
-- deploy fails fast instead of queueing behind a long transaction.

SET LOCAL lock_timeout = '5s';

CREATE SCHEMA IF NOT EXISTS config;

CREATE TABLE IF NOT EXISTS config.change_log (
    id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    changed_at       timestamptz NOT NULL DEFAULT now(),
    xact_id          xid8        NOT NULL DEFAULT pg_current_xact_id(),
    subject_kind     text        NOT NULL
                     CHECK (subject_kind IN ('setting', 'toggle', 'steerer', 'build', 'promotion')),
    subject_key      text        NOT NULL,
    op               text        NOT NULL
                     CHECK (op IN ('insert', 'update', 'delete', 'soft_delete', 'restore', 'observe')),
    old_value        jsonb,
    new_value        jsonb,
    redacted         boolean     NOT NULL DEFAULT false,
    via              text        NOT NULL
                     CHECK (via IN ('spa', 'api', 'startup', 'migration',
                                    'promotion', 'mike', 'sql')),
    actor            text,
    request_id       text,
    build_sha        text,
    db_user          text        NOT NULL DEFAULT current_user,
    application_name text,
    detail           jsonb       NOT NULL DEFAULT '{}'::jsonb
);

COMMENT ON TABLE config.change_log IS
    'Append-only timeline of configuration changes (settings, feature toggles, steerers), builds and promotions. Written by triggers on the three tables and by mike for build and promotion rows. Never updated or deleted.';
COMMENT ON COLUMN config.change_log.xact_id IS
    'Writing transaction. A writer reads back the rows its own transaction produced with WHERE xact_id = pg_current_xact_id(); JC06 stores those ids in eval.bundles.cause.';
COMMENT ON COLUMN config.change_log.via IS
    'Surface of the change. spa = a browser request carrying Origin; api = a REST call without Origin (mike-mcp calls included); startup = a build row; migration = a migration or the baseline; promotion = a gate or promotion act; mike = a mike write with no explicit source; sql = any other database client (psql). A provenance value outside this list falls back to the application_name default.';
COMMENT ON COLUMN config.change_log.redacted IS
    'True when the key is secret-shaped per config.is_secret_setting_key; old_value and new_value are then {"redacted": true}.';

CREATE INDEX IF NOT EXISTS change_log_subject_idx
    ON config.change_log (subject_kind, subject_key, id DESC);
CREATE INDEX IF NOT EXISTS change_log_changed_at_idx
    ON config.change_log (changed_at DESC, id DESC);

CREATE OR REPLACE FUNCTION config.is_secret_setting_key(k text) RETURNS boolean
LANGUAGE sql IMMUTABLE AS $$
    SELECT k !~ '_env$'
       AND EXISTS (
           SELECT 1
             FROM regexp_split_to_table(k, '[._]') AS seg
            WHERE seg IN ('key', 'secret', 'token', 'password', 'credential')
       )
$$;

CREATE OR REPLACE FUNCTION config.default_change_via() RETURNS text
LANGUAGE sql STABLE AS $$
    SELECT CASE current_setting('application_name', true)
               WHEN 'claude-proxy'         THEN 'mike'
               WHEN 'claude-proxy-migrate' THEN 'migration'
               ELSE 'sql'
           END
$$;

CREATE OR REPLACE FUNCTION config.current_change_source() RETURNS jsonb
LANGUAGE plpgsql STABLE AS $$
DECLARE
    raw      text := current_setting('mike.change_source', true);
    fallback jsonb := jsonb_build_object('via', config.default_change_via());
BEGIN
    IF raw IS NULL OR raw = '' THEN
        RETURN fallback;
    END IF;
    BEGIN
        RETURN raw::jsonb;
    EXCEPTION WHEN others THEN
        RETURN fallback || jsonb_build_object('detail', jsonb_build_object('source_error', raw));
    END;
END $$;

-- A provenance value whose via is missing or outside the CHECK list must not fail the
-- configuration write it describes: it falls back to the default attribution and the
-- rejected value is kept in detail.source_error. Keep this list equal to the CHECK.
CREATE OR REPLACE FUNCTION config.append_change(
    p_kind text, p_key text, p_op text, p_old jsonb, p_new jsonb, p_redacted boolean
) RETURNS void
LANGUAGE plpgsql AS $$
DECLARE
    src      jsonb := config.current_change_source();
    v_via    text  := src->>'via';
    v_detail jsonb := CASE jsonb_typeof(src->'detail') WHEN 'object' THEN src->'detail'
                                                     ELSE '{}'::jsonb END;
BEGIN
    IF v_via IS NULL OR v_via NOT IN ('spa', 'api', 'startup', 'migration', 'promotion', 'mike', 'sql') THEN
        v_detail := v_detail || jsonb_build_object('source_error', src);
        v_via    := config.default_change_via();
    END IF;
    INSERT INTO config.change_log
        (subject_kind, subject_key, op, old_value, new_value, redacted,
         via, actor, request_id, build_sha, application_name, detail)
    VALUES
        (p_kind, p_key, p_op, p_old, p_new, p_redacted,
         v_via, src->>'actor', src->>'request_id', src->>'build_sha',
         current_setting('application_name', true),
         v_detail);
END $$;

CREATE OR REPLACE FUNCTION config.log_setting_change() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    k      text  := CASE WHEN TG_OP = 'DELETE' THEN OLD.key ELSE NEW.key END;
    old_v  jsonb := CASE WHEN TG_OP = 'INSERT' THEN NULL ELSE OLD.value END;
    new_v  jsonb := CASE WHEN TG_OP = 'DELETE' THEN NULL ELSE NEW.value END;
    secret boolean := config.is_secret_setting_key(k);
BEGIN
    IF TG_OP = 'UPDATE' AND old_v IS NOT DISTINCT FROM new_v THEN
        RETURN NULL;
    END IF;
    PERFORM config.append_change(
        'setting', k, lower(TG_OP),
        CASE WHEN secret AND old_v IS NOT NULL THEN '{"redacted": true}'::jsonb ELSE old_v END,
        CASE WHEN secret AND new_v IS NOT NULL THEN '{"redacted": true}'::jsonb ELSE new_v END,
        secret);
    RETURN NULL;
END $$;

CREATE OR REPLACE FUNCTION config.log_toggle_change() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    k     text  := CASE WHEN TG_OP = 'DELETE' THEN OLD.key ELSE NEW.key END;
    old_v jsonb := CASE WHEN TG_OP = 'INSERT' THEN NULL
                        ELSE jsonb_build_object('enabled', OLD.enabled, 'operator_set', OLD.operator_set) END;
    new_v jsonb := CASE WHEN TG_OP = 'DELETE' THEN NULL
                        ELSE jsonb_build_object('enabled', NEW.enabled, 'operator_set', NEW.operator_set) END;
BEGIN
    IF TG_OP = 'UPDATE' AND old_v IS NOT DISTINCT FROM new_v THEN
        RETURN NULL;
    END IF;
    PERFORM config.append_change('toggle', k, lower(TG_OP), old_v, new_v, false);
    RETURN NULL;
END $$;

CREATE OR REPLACE FUNCTION config.log_steerer_change() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    sid   bigint := CASE WHEN TG_OP = 'DELETE' THEN OLD.id ELSE NEW.id END;
    old_v jsonb  := CASE WHEN TG_OP = 'INSERT' THEN NULL ELSE to_jsonb(OLD) - 'updated_at' END;
    new_v jsonb  := CASE WHEN TG_OP = 'DELETE' THEN NULL ELSE to_jsonb(NEW) - 'updated_at' END;
    op    text   := lower(TG_OP);
BEGIN
    IF TG_OP = 'UPDATE' THEN
        IF old_v IS NOT DISTINCT FROM new_v THEN
            RETURN NULL;
        END IF;
        IF OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL THEN
            op := 'soft_delete';
        ELSIF OLD.deleted_at IS NOT NULL AND NEW.deleted_at IS NULL THEN
            op := 'restore';
        END IF;
    END IF;
    PERFORM config.append_change('steerer', sid::text, op, old_v, new_v, false);
    RETURN NULL;
END $$;

DROP TRIGGER IF EXISTS trg_settings_change_log ON config.settings;
CREATE TRIGGER trg_settings_change_log
    AFTER INSERT OR UPDATE OR DELETE ON config.settings
    FOR EACH ROW EXECUTE FUNCTION config.log_setting_change();

DROP TRIGGER IF EXISTS trg_feature_toggles_change_log ON config.feature_toggles;
CREATE TRIGGER trg_feature_toggles_change_log
    AFTER INSERT OR UPDATE OR DELETE ON config.feature_toggles
    FOR EACH ROW EXECUTE FUNCTION config.log_toggle_change();

DROP TRIGGER IF EXISTS trg_steerers_change_log ON steering.steerers;
CREATE TRIGGER trg_steerers_change_log
    AFTER INSERT OR UPDATE OR DELETE ON steering.steerers
    FOR EACH ROW EXECUTE FUNCTION config.log_steerer_change();

-- Baseline: the state at migration time, one 'observe' row per existing row, so a
-- snapshot as of any later moment is answerable. The row's previous updated_at is
-- kept in detail; the values it replaced were never stored and are not recoverable.
INSERT INTO config.change_log
    (subject_kind, subject_key, op, old_value, new_value, redacted, via, application_name, detail)
SELECT 'setting', s.key, 'observe', NULL,
       CASE WHEN config.is_secret_setting_key(s.key) THEN '{"redacted": true}'::jsonb ELSE s.value END,
       config.is_secret_setting_key(s.key), 'migration', current_setting('application_name', true),
       jsonb_build_object('baseline', true, 'row_updated_at', s.updated_at)
  FROM config.settings s;

INSERT INTO config.change_log
    (subject_kind, subject_key, op, old_value, new_value, redacted, via, application_name, detail)
SELECT 'toggle', t.key, 'observe', NULL,
       jsonb_build_object('enabled', t.enabled, 'operator_set', t.operator_set),
       false, 'migration', current_setting('application_name', true),
       jsonb_build_object('baseline', true, 'row_updated_at', t.updated_at)
  FROM config.feature_toggles t;

INSERT INTO config.change_log
    (subject_kind, subject_key, op, old_value, new_value, redacted, via, application_name, detail)
SELECT 'steerer', st.id::text, 'observe', NULL,
       to_jsonb(st) - 'updated_at',
       false, 'migration', current_setting('application_name', true),
       jsonb_build_object('baseline', true, 'row_updated_at', st.updated_at)
  FROM steering.steerers st;
