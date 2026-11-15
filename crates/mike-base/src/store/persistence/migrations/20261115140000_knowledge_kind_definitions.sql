-- A kind of project knowledge is defined, changed and deleted at run time (issue #284). The definition columns on
-- memory.knowledge_kinds let the settings page show how a kind was made; the functions below validate a definition,
-- create its view, watch and triggers, and refuse with SQLSTATE MK400 and a message naming what is wrong.
-- Embedding state now carries the item's name, label and a text vector, so search reads a kind it has never heard of
-- from the state rows instead of from a per-kind query.

ALTER TABLE memory.knowledge_kinds
    ADD COLUMN IF NOT EXISTS label text,
    ADD COLUMN IF NOT EXISTS description text,
    ADD COLUMN IF NOT EXISTS generated_view boolean NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS source_table text,
    ADD COLUMN IF NOT EXISTS key_column text,
    ADD COLUMN IF NOT EXISTS title_column text,
    ADD COLUMN IF NOT EXISTS body_columns text[],
    ADD COLUMN IF NOT EXISTS project_column text;

UPDATE memory.knowledge_kinds SET label = kind WHERE label IS NULL;

ALTER TABLE memory.knowledge_kinds
    ALTER COLUMN label SET NOT NULL,
    DROP CONSTRAINT IF EXISTS knowledge_kinds_definition_shape_check,
    ADD CONSTRAINT knowledge_kinds_definition_shape_check CHECK (
        generated_view = (source_table IS NOT NULL)
        AND (source_table IS NULL) = (key_column IS NULL)
        AND (source_table IS NULL) = (title_column IS NULL)
        AND (source_table IS NULL) = (body_columns IS NULL)
        AND (source_table IS NULL) = (project_column IS NULL)
        AND NOT (built_in AND generated_view));

COMMENT ON COLUMN memory.knowledge_kinds.label IS
    'Display name of the kind; for a generated view it is also the label every item of the kind carries.';
COMMENT ON COLUMN memory.knowledge_kinds.generated_view IS
    'True when memory.define_knowledge_kind created body_view from source_table and the column names below; false for built-in kinds and for kinds that name their own view.';
COMMENT ON COLUMN memory.knowledge_kinds.source_table IS
    'Qualified table a generated view reads; NULL unless generated_view.';

ALTER TABLE memory.knowledge_embedding_state
    ADD COLUMN IF NOT EXISTS name text,
    ADD COLUMN IF NOT EXISTS label text,
    ADD COLUMN IF NOT EXISTS search_tsv tsvector;

COMMENT ON COLUMN memory.knowledge_embedding_state.search_tsv IS
    'Text vector of header and body, written when the item is embedded and only for kinds that are not built in; built-in kinds are searched through their own tables.';

CREATE INDEX IF NOT EXISTS idx_knowledge_embedding_state_reference
    ON memory.knowledge_embedding_state (reference);
CREATE INDEX IF NOT EXISTS idx_knowledge_embedding_state_search_tsv
    ON memory.knowledge_embedding_state USING gin (search_tsv);

ALTER TABLE memory.knowledge_items ADD COLUMN IF NOT EXISTS last_error_at timestamptz;

COMMENT ON COLUMN memory.knowledge_items.last_error_at IS
    'When last_error was recorded; NULL for errors recorded before this column existed.';

CREATE INDEX IF NOT EXISTS idx_knowledge_items_kind_project
    ON memory.knowledge_items (kind, project_id, changed_at DESC);

CREATE OR REPLACE FUNCTION memory.refuse_knowledge_kind(p_message text, p_field text DEFAULT NULL) RETURNS void
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION '%', p_message USING ERRCODE = 'MK400', DETAIL = p_field;
END
$$;

CREATE OR REPLACE FUNCTION memory.knowledge_regclass(p_name text) RETURNS regclass
LANGUAGE plpgsql STABLE AS $$
BEGIN
    RETURN to_regclass(p_name);
EXCEPTION WHEN syntax_error OR invalid_name THEN
    RETURN NULL;
END
$$;

CREATE OR REPLACE FUNCTION memory.knowledge_column_type(p_relation regclass, p_column text) RETURNS text
LANGUAGE sql STABLE AS $$
    SELECT a.atttypid::regtype::text
      FROM pg_attribute a
     WHERE a.attrelid = p_relation AND a.attname = p_column AND a.attnum > 0 AND NOT a.attisdropped
$$;

DROP FUNCTION IF EXISTS memory.require_knowledge_columns(regclass, text, text[]);

CREATE OR REPLACE FUNCTION memory.require_knowledge_columns(
    p_relation regclass, p_name text, p_columns text[], p_field text)
RETURNS void
LANGUAGE plpgsql AS $$
DECLARE
    v_missing text;
BEGIN
    IF array_position(p_columns, NULL) IS NOT NULL THEN
        PERFORM memory.refuse_knowledge_kind('a column name is empty', p_field);
    END IF;
    SELECT u.c INTO v_missing
      FROM unnest(p_columns) WITH ORDINALITY u(c, n)
     WHERE memory.knowledge_column_type(p_relation, u.c) IS NULL
     ORDER BY u.n
     LIMIT 1;
    IF FOUND THEN
        PERFORM memory.refuse_knowledge_kind(format('column %s is not on %s', v_missing, p_name), p_field);
    END IF;
END
$$;

CREATE OR REPLACE FUNCTION memory.qualified_knowledge_relation(p_relation regclass) RETURNS text
LANGUAGE sql STABLE AS $$
    SELECT format('%I.%I', n.nspname, c.relname)
      FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
     WHERE c.oid = p_relation
$$;

CREATE OR REPLACE FUNCTION memory.create_generated_knowledge_view(p_kind text) RETURNS void
LANGUAGE plpgsql AS $$
DECLARE
    k memory.knowledge_kinds;
    v_table regclass;
    v_join text;
    v_body text;
BEGIN
    SELECT * INTO STRICT k FROM memory.knowledge_kinds WHERE kind = p_kind AND generated_view;
    v_table := memory.knowledge_regclass(k.source_table);
    IF v_table IS NULL THEN
        PERFORM memory.refuse_knowledge_kind(format('table %s does not exist', k.source_table), 'source_table');
    END IF;
    v_join := CASE memory.knowledge_column_type(v_table, k.project_column)
        WHEN 'uuid' THEN format('JOIN projects.projects p ON p.project_uuid = t.%I', k.project_column)
        ELSE format('JOIN projects.projects p ON p.id = t.%I', k.project_column)
    END;
    SELECT format('concat_ws(chr(10) || chr(10), %s)', string_agg(format('t.%I::text', u.c), ', ' ORDER BY u.n))
      INTO v_body
      FROM unnest(ARRAY[k.title_column] || k.body_columns) WITH ORDINALITY u(c, n);
    EXECUTE format(
        $f$CREATE OR REPLACE VIEW memory.%I AS
           SELECT s.source_id, s.project_id, s.reference, s.label, s.name, s.body
             FROM (SELECT t.%I::bigint AS source_id, p.id AS project_id,
                          %L || ':' || t.%I::text AS reference,
                          %L::text AS label, t.%I::text AS name, %s AS body
                     FROM %s t %s) s
            WHERE length(btrim(s.body)) > 0$f$,
        'knowledge_body_' || k.kind || '_v', k.key_column, k.kind, k.key_column, k.label,
        k.title_column, v_body, v_table, v_join);
END
$$;

CREATE OR REPLACE FUNCTION memory.define_knowledge_kind(
    p_kind text, p_label text, p_description text, p_table text, p_key_column text, p_title_column text,
    p_body_columns text[], p_project_column text, p_watch_columns text[], p_settle_secs integer, p_view text)
RETURNS text
LANGUAGE plpgsql AS $$
DECLARE
    v_generated boolean := p_view IS NULL;
    v_table regclass;
    v_view regclass;
    v_view_columns text[] := ARRAY['source_id', 'project_id', 'reference', 'label', 'name', 'body'];
    v_missing text;
    v_relkind "char";
    v_schema name;
    v_relname name;
    v_type text;
    v_label text;
    v_settle integer := COALESCE(p_settle_secs, 0);
    v_watch_columns text[];
    v_body_view text;
    v_table_name text;
    v_bad_count bigint;
    v_bad_example text;
    v_watch bigint;
BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended('memory.knowledge_kinds', 0));
    IF p_kind IS NULL OR p_kind !~ '^[a-z][a-z0-9_]{0,39}$' THEN
        PERFORM memory.refuse_knowledge_kind(
            'kind name must start with a lowercase letter and use only lowercase letters, digits and underscores, at most 40 characters',
            'kind');
    END IF;
    IF p_kind = ANY (ARRAY['launch', 'turn', 'promise', 'work_thread', 'memory', 'skill', 'seat']) THEN
        PERFORM memory.refuse_knowledge_kind(format('kind %s is reserved for a source mike reads by itself', p_kind), 'kind');
    END IF;
    IF EXISTS (SELECT 1 FROM memory.knowledge_kinds k WHERE k.kind = p_kind) THEN
        PERFORM memory.refuse_knowledge_kind(format('kind %s already exists', p_kind), 'kind');
    END IF;
    IF v_settle NOT BETWEEN 0 AND 86400 THEN
        PERFORM memory.refuse_knowledge_kind('settle seconds must be between 0 and 86400', 'settle_secs');
    END IF;
    IF btrim(COALESCE(p_table, '')) = '' THEN
        PERFORM memory.refuse_knowledge_kind('a table is required', 'source_table');
    END IF;
    IF btrim(COALESCE(p_key_column, '')) = '' THEN
        PERFORM memory.refuse_knowledge_kind('a key column is required', 'key_column');
    END IF;
    IF v_generated AND btrim(COALESCE(p_title_column, '')) = '' THEN
        PERFORM memory.refuse_knowledge_kind('a title column is required; or name a view', 'title_column');
    END IF;
    IF v_generated AND p_body_columns IS NULL THEN
        PERFORM memory.refuse_knowledge_kind('body columns are required; or name a view', 'body_columns');
    END IF;
    IF v_generated AND btrim(COALESCE(p_project_column, '')) = '' THEN
        PERFORM memory.refuse_knowledge_kind('a project column is required; or name a view', 'project_column');
    END IF;
    IF NOT v_generated AND (p_title_column IS NOT NULL OR p_body_columns IS NOT NULL OR p_project_column IS NOT NULL) THEN
        PERFORM memory.refuse_knowledge_kind(
            'a kind that names a view takes its text from the view; leave title_column, body_columns and project_column empty',
            'view');
    END IF;
    v_label := COALESCE(NULLIF(btrim(p_label), ''), p_kind);

    v_table := memory.knowledge_regclass(p_table);
    IF v_table IS NULL THEN
        PERFORM memory.refuse_knowledge_kind(format('table %s does not exist', p_table), 'source_table');
    END IF;
    SELECT c.relkind, n.nspname, c.relname INTO v_relkind, v_schema, v_relname
      FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.oid = v_table;
    IF v_relkind NOT IN ('r', 'p') THEN
        PERFORM memory.refuse_knowledge_kind(
            format('%s is not a table; triggers can only watch tables', p_table), 'source_table');
    END IF;
    IF v_schema IN ('pg_catalog', 'information_schema') OR (v_schema = 'memory' AND v_relname LIKE 'knowledge\_%') THEN
        PERFORM memory.refuse_knowledge_kind(
            format('table %s belongs to the database or to mike''s knowledge bookkeeping and cannot be a source', p_table),
            'source_table');
    END IF;
    v_table_name := memory.qualified_knowledge_relation(v_table);
    PERFORM memory.require_knowledge_columns(v_table, p_table, ARRAY[p_key_column], 'key_column');
    v_type := memory.knowledge_column_type(v_table, p_key_column);
    IF v_type NOT IN ('smallint', 'integer', 'bigint') THEN
        PERFORM memory.refuse_knowledge_kind(
            format('key column %s on %s is %s; it must be smallint, integer or bigint', p_key_column, p_table, v_type),
            'key_column');
    END IF;

    IF v_generated THEN
        PERFORM memory.require_knowledge_columns(v_table, p_table, ARRAY[p_title_column], 'title_column');
        PERFORM memory.require_knowledge_columns(v_table, p_table, p_body_columns, 'body_columns');
        PERFORM memory.require_knowledge_columns(v_table, p_table, ARRAY[p_project_column], 'project_column');
        v_type := memory.knowledge_column_type(v_table, p_project_column);
        IF v_type NOT IN ('smallint', 'integer', 'bigint', 'uuid') THEN
            PERFORM memory.refuse_knowledge_kind(
                format('project column %s on %s is %s; it must be bigint or uuid', p_project_column, p_table, v_type),
                'project_column');
        END IF;
        v_body_view := format('memory.%I', 'knowledge_body_' || p_kind || '_v');
        v_watch_columns := CASE WHEN COALESCE(cardinality(p_watch_columns), 0) > 0 THEN p_watch_columns
            ELSE (SELECT array_agg(DISTINCT c ORDER BY c)
                    FROM unnest(ARRAY[p_key_column, p_title_column] || p_body_columns || ARRAY[p_project_column]) c) END;
    ELSE
        v_view := memory.knowledge_regclass(p_view);
        IF v_view IS NULL THEN
            PERFORM memory.refuse_knowledge_kind(format('view %s does not exist', p_view), 'view');
        END IF;
        IF NOT EXISTS (SELECT 1 FROM pg_class c WHERE c.oid = v_view AND c.relkind IN ('v', 'm')) THEN
            PERFORM memory.refuse_knowledge_kind(format('%s is not a view', p_view), 'view');
        END IF;
        SELECT c INTO v_missing FROM unnest(v_view_columns) WITH ORDINALITY u(c, n)
         WHERE memory.knowledge_column_type(v_view, u.c) IS NULL ORDER BY u.n LIMIT 1;
        IF FOUND THEN
            PERFORM memory.refuse_knowledge_kind(format(
                'view %s must return source_id, project_id, reference, label, name, body; it lacks %s', p_view, v_missing),
                'view');
        END IF;
        IF memory.knowledge_column_type(v_view, 'source_id') NOT IN ('smallint', 'integer', 'bigint')
           OR memory.knowledge_column_type(v_view, 'project_id') NOT IN ('smallint', 'integer', 'bigint') THEN
            PERFORM memory.refuse_knowledge_kind(
                format('view %s must return source_id and project_id as integers', p_view), 'view');
        END IF;
        EXECUTE format(
            'SELECT count(*), min(b.reference::text)
               FROM (SELECT reference FROM %s LIMIT 200) b
              WHERE NOT COALESCE(starts_with(b.reference::text, %L), false)',
            v_view, p_kind || ':')
           INTO v_bad_count, v_bad_example;
        IF v_bad_count > 0 THEN
            PERFORM memory.refuse_knowledge_kind(format(
                'view %s must return references that start with %s; %s of its first 200 rows do not (for example %s)',
                p_view, p_kind || ':', v_bad_count, COALESCE(v_bad_example, 'an empty reference')),
                'view');
        END IF;
        v_body_view := memory.qualified_knowledge_relation(v_view);
        v_watch_columns := COALESCE(p_watch_columns, '{}'::text[]);
    END IF;
    IF cardinality(v_watch_columns) > 0 THEN
        PERFORM memory.require_knowledge_columns(v_table, p_table, v_watch_columns, 'watch_columns');
    END IF;

    INSERT INTO memory.knowledge_kinds
        (kind, body_view, settle_secs, built_in, label, description, generated_view,
         source_table, key_column, title_column, body_columns, project_column)
    VALUES (p_kind, v_body_view, v_settle, false, v_label, NULLIF(btrim(COALESCE(p_description, '')), ''), v_generated,
            CASE WHEN v_generated THEN v_table_name END, CASE WHEN v_generated THEN p_key_column END,
            p_title_column, p_body_columns, p_project_column);
    IF v_generated THEN
        PERFORM memory.create_generated_knowledge_view(p_kind);
    END IF;
    INSERT INTO memory.knowledge_kind_watches (kind, watched_table, key_column, update_columns)
    VALUES (p_kind, v_table_name, p_key_column, v_watch_columns)
    RETURNING id INTO v_watch;
    PERFORM memory.attach_knowledge_watch(v_watch);
    PERFORM memory.reconcile_knowledge_items(p_kind);
    RETURN p_kind;
END
$$;

CREATE OR REPLACE FUNCTION memory.update_knowledge_kind(
    p_kind text, p_enabled boolean, p_settle_secs integer, p_watch_columns text[], p_label text, p_description text)
RETURNS void
LANGUAGE plpgsql AS $$
DECLARE
    k memory.knowledge_kinds;
    w memory.knowledge_kind_watches;
    v_watches bigint;
    v_label text;
BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended('memory.knowledge_kinds', 0));
    SELECT * INTO k FROM memory.knowledge_kinds WHERE kind = p_kind FOR UPDATE;
    IF NOT FOUND THEN
        PERFORM memory.refuse_knowledge_kind(format('kind %s does not exist', p_kind), 'kind');
    END IF;
    IF p_settle_secs IS NOT NULL AND p_settle_secs NOT BETWEEN 0 AND 86400 THEN
        PERFORM memory.refuse_knowledge_kind('settle seconds must be between 0 and 86400', 'settle_secs');
    END IF;
    v_label := NULLIF(btrim(COALESCE(p_label, '')), '');
    IF p_watch_columns IS NOT NULL THEN
        IF cardinality(p_watch_columns) = 0 THEN
            PERFORM memory.refuse_knowledge_kind('watch columns must name at least one column', 'watch_columns');
        END IF;
        SELECT count(*) INTO v_watches FROM memory.knowledge_kind_watches WHERE kind = p_kind;
        IF v_watches <> 1 THEN
            PERFORM memory.refuse_knowledge_kind(format(
                'kind %s watches %s tables; watch columns can only be changed on a kind with one watch', p_kind, v_watches),
                'watch_columns');
        END IF;
        SELECT * INTO STRICT w FROM memory.knowledge_kind_watches WHERE kind = p_kind;
        IF memory.knowledge_regclass(w.watched_table) IS NULL THEN
            PERFORM memory.refuse_knowledge_kind(format('table %s does not exist', w.watched_table), 'watch_columns');
        END IF;
        PERFORM memory.require_knowledge_columns(
            memory.knowledge_regclass(w.watched_table), w.watched_table, p_watch_columns, 'watch_columns');
        UPDATE memory.knowledge_kind_watches SET update_columns = p_watch_columns WHERE id = w.id;
        PERFORM memory.attach_knowledge_watch(w.id);
    END IF;
    UPDATE memory.knowledge_kinds
       SET enabled = COALESCE(p_enabled, enabled),
           settle_secs = COALESCE(p_settle_secs, settle_secs),
           label = COALESCE(v_label, label),
           description = CASE WHEN p_description IS NULL THEN description ELSE NULLIF(btrim(p_description), '') END
     WHERE kind = p_kind;
    IF v_label IS NOT NULL AND v_label <> k.label AND k.generated_view THEN
        PERFORM memory.create_generated_knowledge_view(p_kind);
    END IF;
    IF p_enabled AND NOT k.enabled THEN
        PERFORM memory.reconcile_knowledge_items(p_kind);
    END IF;
END
$$;

CREATE OR REPLACE FUNCTION memory.delete_knowledge_kind(p_kind text) RETURNS void
LANGUAGE plpgsql AS $$
DECLARE
    k memory.knowledge_kinds;
BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended('memory.knowledge_kinds', 0));
    SELECT * INTO k FROM memory.knowledge_kinds WHERE kind = p_kind FOR UPDATE;
    IF NOT FOUND THEN
        PERFORM memory.refuse_knowledge_kind(format('kind %s does not exist', p_kind), 'kind');
    END IF;
    IF k.built_in THEN
        PERFORM memory.refuse_knowledge_kind(format('knowledge kind %s is built in; switch it off instead', p_kind), 'kind');
    END IF;
    PERFORM memory.detach_knowledge_watch(w.id) FROM memory.knowledge_kind_watches w WHERE w.kind = p_kind;
    DELETE FROM memory.knowledge_kinds WHERE kind = p_kind;
    IF k.generated_view THEN
        EXECUTE format('DROP VIEW IF EXISTS memory.%I', 'knowledge_body_' || p_kind || '_v');
    END IF;
END
$$;

CREATE OR REPLACE FUNCTION memory.knowledge_kind_stats()
RETURNS TABLE (kind text, items bigint, embedded bigint, waiting bigint, failed bigint,
               newest_error text, newest_error_at timestamptz, oldest_waiting_ready_at timestamptz)
LANGUAGE sql STABLE AS $$
    WITH totals AS (
        SELECT i.kind, count(*) AS items FROM memory.knowledge_items i GROUP BY i.kind
    ), waiting AS (
        SELECT i.kind, count(*) AS waiting, count(*) FILTER (WHERE i.last_error IS NOT NULL) AS waiting_failed,
               min(i.ready_at) AS oldest
          FROM memory.knowledge_items i WHERE i.needs_embedding GROUP BY i.kind
    ), failed AS (
        SELECT i.kind, count(*) AS failed FROM memory.knowledge_items i WHERE i.last_error IS NOT NULL GROUP BY i.kind
    ), newest AS (
        SELECT DISTINCT ON (i.kind) i.kind, i.last_error, COALESCE(i.last_error_at, i.changed_at) AS err_at
          FROM memory.knowledge_items i WHERE i.last_error IS NOT NULL
         ORDER BY i.kind, COALESCE(i.last_error_at, i.changed_at) DESC, i.id DESC
    )
    SELECT k.kind, COALESCE(t.items, 0)::bigint,
           (COALESCE(t.items, 0) - COALESCE(w.waiting, 0) - COALESCE(f.failed, 0) + COALESCE(w.waiting_failed, 0))::bigint,
           COALESCE(w.waiting, 0)::bigint, COALESCE(f.failed, 0)::bigint,
           n.last_error, n.err_at, w.oldest
      FROM memory.knowledge_kinds k
      LEFT JOIN totals t ON t.kind = k.kind
      LEFT JOIN waiting w ON w.kind = k.kind
      LEFT JOIN failed f ON f.kind = k.kind
      LEFT JOIN newest n ON n.kind = k.kind
     ORDER BY k.kind
$$;
