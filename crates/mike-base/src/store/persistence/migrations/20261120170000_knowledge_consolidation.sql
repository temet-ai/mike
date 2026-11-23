-- Knowledge consolidation (design: mike-artifacts/design/2026-11-20-knowledge-consolidation.md).
--
-- Every item mike indexes can be queued for consolidation. A file item is split into single statements,
-- a lesson is one statement, statements are grouped into subjects by meaning, a subject is judged as a
-- group and each judgement is recorded as a decision. Twins (the same file, byte for byte, in several
-- checkouts of one project or on one machine) own their statements once, through the lowest item id of
-- the twin group; the other twin items resolve to it with memory.knowledge_twin_canonical.

ALTER TABLE memory.knowledge_items ADD COLUMN IF NOT EXISTS consolidate_at timestamptz;

COMMENT ON COLUMN memory.knowledge_items.consolidate_at IS
    'When the item was last queued for consolidation: set when it is saved with a changed source hash. NULL means nothing to do. A worker clears it only if it is still the value it claimed, so a change during processing queues the item again.';

CREATE VIEW memory.knowledge_artifact_identity_v AS
SELECT r.id AS revision_id, r.title, r.content_hash, d.relative_path, s.name AS source_name,
       CASE WHEN s.machine_uuid IS NOT NULL THEN 'machine'
            ELSE 'project:' || COALESCE(loc.project_id::text, '') END AS scope,
       (d.current_revision = r.id AND d.deleted_at IS NULL) AS current
  FROM memory.knowledge_revisions r
  JOIN memory.knowledge_documents d ON d.document_uuid = r.document_uuid
  JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
  LEFT JOIN projects.locations loc ON loc.location_uuid = s.location_uuid;

COMMENT ON VIEW memory.knowledge_artifact_identity_v IS
    'One row per file revision with the identity that makes two files twins: scope (the location''s project, or machine), source name, relative path, title and content hash. current is true while the revision is its document''s current one.';

CREATE FUNCTION memory.knowledge_artifact_twins(p_revision_ids bigint[])
RETURNS TABLE (revision_id bigint, twin_item_id bigint)
LANGUAGE sql STABLE AS $$
    SELECT a.revision_id, j.id
      FROM memory.knowledge_artifact_identity_v a
      JOIN memory.knowledge_artifact_identity_v b
        ON b.current AND b.scope = a.scope AND b.source_name = a.source_name
       AND b.relative_path = a.relative_path AND b.title = a.title AND b.content_hash = a.content_hash
      JOIN memory.knowledge_items j ON j.kind = 'artifact' AND j.source_id = b.revision_id
     WHERE a.revision_id = ANY(p_revision_ids)
$$;

COMMENT ON FUNCTION memory.knowledge_artifact_twins(bigint[]) IS
    'For each given file revision, the items of every current file with the same twin identity (the revision''s own item included while it is current).';

CREATE FUNCTION memory.knowledge_twin_canonical(p_item_ids bigint[])
RETURNS TABLE (item_id bigint, canonical_item_id bigint)
LANGUAGE sql STABLE AS $$
    WITH picked AS (
        SELECT i.id, i.source_id FROM memory.knowledge_items i WHERE i.id = ANY(p_item_ids) AND i.kind = 'artifact'
    ), tw AS (
        SELECT t.revision_id, t.twin_item_id
          FROM memory.knowledge_artifact_twins(ARRAY(SELECT p.source_id FROM picked p)) t
    )
    SELECT p.id, COALESCE(min(tw.twin_item_id), p.id)
      FROM picked p
      LEFT JOIN tw ON tw.revision_id = p.source_id
     GROUP BY p.id
    UNION ALL
    SELECT i.id, i.id FROM memory.knowledge_items i WHERE i.id = ANY(p_item_ids) AND i.kind <> 'artifact'
$$;

COMMENT ON FUNCTION memory.knowledge_twin_canonical(bigint[]) IS
    'The item that owns the statements of each given item: the lowest item id among its twins, or the item itself when it has none or is not a file.';

CREATE FUNCTION memory.queue_knowledge_consolidation(p_item_ids bigint[]) RETURNS bigint
LANGUAGE sql AS $$
    WITH queued AS (
        UPDATE memory.knowledge_items SET consolidate_at = clock_timestamp()
         WHERE id = ANY(p_item_ids)
        RETURNING 1
    )
    SELECT count(*) FROM queued
$$;

COMMENT ON FUNCTION memory.queue_knowledge_consolidation(bigint[]) IS
    'Queues items for consolidation. Called when an item is saved with a changed source hash and when twins are served, whatever the kind; the worker decides per kind what to do.';

UPDATE memory.knowledge_items i SET consolidate_at = now()
  FROM memory.lessons l
 WHERE i.kind = 'lesson' AND l.id = i.source_id AND l.archived_at IS NULL AND l.retired_at IS NULL
   AND i.consolidate_at IS NULL;

UPDATE memory.knowledge_items i SET consolidate_at = now()
  FROM memory.knowledge_artifact_identity_v a
 WHERE i.kind = 'artifact' AND a.revision_id = i.source_id AND a.current
   AND a.source_name IN ('claude-memory', 'skills', 'rules', 'agents', 'agent-memory', 'instructions')
   AND i.consolidate_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_knowledge_items_consolidation_waiting
    ON memory.knowledge_items (consolidate_at, id) WHERE consolidate_at IS NOT NULL;

CREATE OR REPLACE FUNCTION memory.share_twin_embeddings(p_item_ids bigint[], p_model_key text)
RETURNS bigint[]
LANGUAGE plpgsql AS $$
DECLARE
    v_items bigint[];
    v_twins bigint[];
    v_served bigint[];
BEGIN
    SELECT array_agg(t.item_id ORDER BY t.item_id), array_agg(t.twin_state_id ORDER BY t.item_id)
      INTO v_items, v_twins
      FROM (
        SELECT DISTINCT ON (m.item_id) m.item_id, twin.id AS twin_state_id
          FROM (
            SELECT i.id AS item_id, d.document_uuid, d.relative_path, s.name AS source_name,
                   s.machine_uuid IS NOT NULL AS machine_scope, loc.project_id AS scope_project,
                   r.title, r.content_hash
              FROM memory.knowledge_items i
              JOIN memory.knowledge_revisions r ON r.id = i.source_id
              JOIN memory.knowledge_documents d
                ON d.document_uuid = r.document_uuid AND d.current_revision = r.id AND d.deleted_at IS NULL
              JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
              LEFT JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
             WHERE i.id = ANY(p_item_ids) AND i.kind = 'artifact' AND i.needs_embedding
          ) m
          JOIN memory.knowledge_sources s2 ON s2.name = m.source_name
          LEFT JOIN projects.locations loc2 ON loc2.location_uuid = s2.location_uuid
          JOIN memory.knowledge_documents d2
            ON d2.source_uuid = s2.source_uuid AND d2.relative_path = m.relative_path
           AND d2.deleted_at IS NULL AND d2.document_uuid <> m.document_uuid
          JOIN memory.knowledge_revisions r2
            ON r2.id = d2.current_revision AND r2.title = m.title AND r2.content_hash = m.content_hash
          JOIN memory.knowledge_embedding_state twin
            ON twin.source_kind = 'artifact' AND twin.source_id = r2.id
           AND twin.model_key = p_model_key AND twin.completed_at IS NOT NULL
         WHERE (m.machine_scope AND s2.machine_uuid IS NOT NULL)
            OR (NOT m.machine_scope AND loc2.project_id = m.scope_project)
         ORDER BY m.item_id, twin.id
      ) t;
    IF v_items IS NULL THEN
        RETURN ARRAY[]::bigint[];
    END IF;
    DELETE FROM memory.knowledge_embedding_chunks c
     USING memory.knowledge_embedding_state e
     WHERE e.item_id = ANY(v_items) AND c.state_id = e.id;
    WITH served AS (
        SELECT u.item_id, u.twin_state_id FROM unnest(v_items, v_twins) AS u(item_id, twin_state_id)
    ), written AS (
        INSERT INTO memory.knowledge_embedding_state
            (item_id, source_kind, source_id, reference, project_id, model_key, completed_at, name, label)
        SELECT i.id, 'artifact', i.source_id, b.reference, b.project_id, p_model_key, now(), b.name, b.label
          FROM served s
          JOIN memory.knowledge_items i ON i.id = s.item_id
          JOIN memory.knowledge_body_artifact_v b ON b.source_id = i.source_id
        ON CONFLICT (item_id) DO UPDATE SET
            reference = EXCLUDED.reference, project_id = EXCLUDED.project_id, model_key = EXCLUDED.model_key,
            completed_at = now(), attempts = 0, last_error = NULL, name = EXCLUDED.name, label = EXCLUDED.label,
            search_tsv = NULL
        RETURNING id, item_id
    ), copied AS (
        INSERT INTO memory.knowledge_embedding_chunks (state_id, ordinal, body, embedding)
        SELECT w.id, c.ordinal, c.body, c.embedding
          FROM written w
          JOIN served s ON s.item_id = w.item_id
          JOIN memory.knowledge_embedding_chunks c ON c.state_id = s.twin_state_id
        RETURNING 1
    )
    SELECT array_agg(w.item_id ORDER BY w.item_id) INTO v_served FROM written w;
    IF v_served IS NULL THEN
        RETURN ARRAY[]::bigint[];
    END IF;
    UPDATE memory.knowledge_items i
       SET needs_embedding = false, attempts = 0, last_error = NULL, model_key = p_model_key,
           consolidate_at = clock_timestamp()
     WHERE i.id = ANY(v_served);
    RETURN v_served;
END
$$;

CREATE TABLE IF NOT EXISTS memory.knowledge_subjects (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    project_id bigint REFERENCES projects.projects (id) ON DELETE CASCADE,
    centre vector(768) NOT NULL,
    title text NOT NULL DEFAULT '',
    text text NOT NULL DEFAULT '',
    members integer NOT NULL DEFAULT 0 CHECK (members >= 0),
    changed_at timestamptz,
    judge_after timestamptz,
    judged_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT knowledge_subjects_changed_shape CHECK ((changed_at IS NULL) = (judge_after IS NULL))
);

COMMENT ON TABLE memory.knowledge_subjects IS
    'A group of statements, from any carriers, about the same thing. project_id NULL is the machine-wide scope (statements from machine sources). centre is the mean of the current members'' vectors. changed_at is NULL once the subject was judged and nothing joined or left since; otherwise judge_after is the earliest time to judge it.';
COMMENT ON COLUMN memory.knowledge_subjects.text IS
    'The concise text for the subject that keeps every instruction the group gives; empty until the first judgement.';

CREATE INDEX IF NOT EXISTS idx_knowledge_subjects_project ON memory.knowledge_subjects (project_id);
CREATE INDEX IF NOT EXISTS idx_knowledge_subjects_due
    ON memory.knowledge_subjects (judge_after, id) WHERE changed_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_knowledge_subjects_centre
    ON memory.knowledge_subjects USING hnsw (centre vector_cosine_ops) WITH (m = 16, ef_construction = 64);

CREATE TABLE IF NOT EXISTS memory.knowledge_statement_states (
    code text PRIMARY KEY CHECK (code ~ '^[a-z][a-z_]{0,29}$')
);

INSERT INTO memory.knowledge_statement_states (code) VALUES
    ('current'), ('covered'), ('superseded'), ('merged'), ('gone')
ON CONFLICT (code) DO NOTHING;

COMMENT ON TABLE memory.knowledge_statement_states IS
    'current: in force. covered: another statement says the same. superseded: a newer or higher-precedence statement replaced it. merged: its detail was folded into another statement. gone: its carrier was split again.';

CREATE TABLE IF NOT EXISTS memory.knowledge_statements (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    item_id bigint REFERENCES memory.knowledge_items (id) ON DELETE CASCADE,
    document_uuid uuid REFERENCES memory.knowledge_documents (document_uuid) ON DELETE SET NULL,
    revision_id bigint REFERENCES memory.knowledge_revisions (id) ON DELETE SET NULL,
    detached_at timestamptz,
    ordinal integer NOT NULL CHECK (ordinal >= 0),
    text text NOT NULL,
    passage_start integer,
    passage_end integer,
    embedding vector(768) NOT NULL,
    model_key text NOT NULL,
    subject_id bigint REFERENCES memory.knowledge_subjects (id) ON DELETE SET NULL,
    state text NOT NULL DEFAULT 'current' REFERENCES memory.knowledge_statement_states (code),
    resolved_by_statement_id bigint REFERENCES memory.knowledge_statements (id) ON DELETE SET NULL,
    stated_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    split_hash text NOT NULL,
    UNIQUE (item_id, ordinal),
    CONSTRAINT knowledge_statements_passage_shape CHECK (
        (passage_start IS NULL) = (passage_end IS NULL)
        AND (passage_start IS NULL OR (passage_start >= 0 AND passage_end >= passage_start))),
    CONSTRAINT knowledge_statements_detached_shape CHECK (item_id IS NOT NULL OR detached_at IS NOT NULL),
    CONSTRAINT knowledge_statements_resolved_shape CHECK (
        resolved_by_statement_id IS NULL OR state IN ('covered', 'superseded', 'merged'))
);

COMMENT ON TABLE memory.knowledge_statements IS
    'One instruction or fact held by an item (a lesson is one statement, a file is split into several). Only the canonical item of a twin group has statements. passage_start and passage_end are character offsets of the source passage in the item''s text, NULL for lessons. split_hash is the item''s source hash when it was split. A re-split marks the old rows gone and numbers the new ones after them; deleting the item deletes its statements, except that a file item deleted because its document moved to a new revision detaches them: item_id becomes NULL and detached_at is set, document_uuid and revision_id say where they came from, and memory.rehome_document_statements gives them to the new revision''s item. Detached statements are not guidance (every read joins the item); memory.sweep_detached_knowledge_statements marks them gone when no revision claims them in time.';
COMMENT ON COLUMN memory.knowledge_statements.document_uuid IS
    'The document of the file the statement was split from; set on insert from the item''s revision, NULL for lessons.';
COMMENT ON COLUMN memory.knowledge_statements.revision_id IS
    'The file revision whose text passage_start and passage_end index (byte offsets of its body); NULL for lessons.';

CREATE INDEX IF NOT EXISTS idx_knowledge_statements_subject ON memory.knowledge_statements (subject_id);
CREATE INDEX IF NOT EXISTS idx_knowledge_statements_document
    ON memory.knowledge_statements (document_uuid) WHERE document_uuid IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_knowledge_statements_revision
    ON memory.knowledge_statements (revision_id) WHERE revision_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_knowledge_statements_detached
    ON memory.knowledge_statements (detached_at) WHERE item_id IS NULL AND state <> 'gone';
CREATE INDEX IF NOT EXISTS idx_knowledge_statements_resolved_by
    ON memory.knowledge_statements (resolved_by_statement_id) WHERE resolved_by_statement_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_knowledge_statements_embedding
    ON memory.knowledge_statements USING hnsw (embedding vector_cosine_ops) WITH (m = 16, ef_construction = 64)
    WHERE state = 'current';

CREATE FUNCTION memory.fill_knowledge_statement_document() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.item_id IS NOT NULL AND NEW.document_uuid IS NULL AND NEW.revision_id IS NULL THEN
        SELECT r.document_uuid, r.id INTO NEW.document_uuid, NEW.revision_id
          FROM memory.knowledge_items i
          JOIN memory.knowledge_revisions r ON r.id = i.source_id
         WHERE i.id = NEW.item_id AND i.kind = 'artifact';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER trg_knowledge_statements_fill_document BEFORE INSERT ON memory.knowledge_statements
    FOR EACH ROW EXECUTE FUNCTION memory.fill_knowledge_statement_document();

CREATE FUNCTION memory.refresh_knowledge_subjects(p_subject_ids bigint[]) RETURNS void
LANGUAGE sql AS $$
    WITH agg AS (
        SELECT s.subject_id, count(*)::integer AS members, avg(s.embedding) AS centre
          FROM memory.knowledge_statements s
         WHERE s.subject_id = ANY(p_subject_ids) AND s.state = 'current'
         GROUP BY s.subject_id
    )
    UPDATE memory.knowledge_subjects k
       SET members = COALESCE(a.members, 0), centre = COALESCE(a.centre, k.centre)
      FROM unnest(p_subject_ids) u(id)
      LEFT JOIN agg a ON a.subject_id = u.id
     WHERE k.id = u.id
$$;

COMMENT ON FUNCTION memory.refresh_knowledge_subjects(bigint[]) IS
    'Recomputes the member count and the centre (mean of the current members'' vectors) of the given subjects. An empty subject keeps its last centre.';

CREATE FUNCTION memory.mark_knowledge_subjects_changed(p_subject_ids bigint[], p_judge_after timestamptz) RETURNS bigint
LANGUAGE sql AS $$
    WITH marked AS (
        UPDATE memory.knowledge_subjects
           SET changed_at = clock_timestamp(), judge_after = GREATEST(COALESCE(judge_after, p_judge_after), p_judge_after)
         WHERE id = ANY(p_subject_ids)
        RETURNING 1
    )
    SELECT count(*) FROM marked
$$;

COMMENT ON FUNCTION memory.mark_knowledge_subjects_changed(bigint[], timestamptz) IS
    'Marks subjects changed (changed_at is the time of the latest change) and pushes judge_after to the later of its current value and the given time, so a subject is judged once its changes settle.';

CREATE FUNCTION memory.settle_deleted_knowledge_statements() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    v_subjects bigint[];
BEGIN
    SELECT array_agg(DISTINCT o.subject_id) INTO v_subjects
      FROM old_rows o WHERE o.subject_id IS NOT NULL AND o.state = 'current';
    IF v_subjects IS NOT NULL THEN
        PERFORM memory.refresh_knowledge_subjects(v_subjects);
        PERFORM memory.mark_knowledge_subjects_changed(v_subjects, now());
    END IF;
    RETURN NULL;
END
$$;

CREATE TRIGGER trg_knowledge_statements_settle_subjects AFTER DELETE ON memory.knowledge_statements
    REFERENCING OLD TABLE AS old_rows FOR EACH STATEMENT
    EXECUTE FUNCTION memory.settle_deleted_knowledge_statements();

CREATE FUNCTION memory.detach_moved_document_statements() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.kind = 'artifact' AND EXISTS (
        SELECT 1 FROM memory.knowledge_revisions r
          JOIN memory.knowledge_documents d ON d.document_uuid = r.document_uuid
         WHERE r.id = OLD.source_id AND d.deleted_at IS NULL AND d.current_revision IS DISTINCT FROM OLD.source_id) THEN
        UPDATE memory.knowledge_statements
           SET item_id = NULL, detached_at = now()
         WHERE item_id = OLD.id AND state <> 'gone' AND document_uuid IS NOT NULL;
    END IF;
    RETURN OLD;
END
$$;

CREATE TRIGGER trg_knowledge_items_detach_statements BEFORE DELETE ON memory.knowledge_items
    FOR EACH ROW EXECUTE FUNCTION memory.detach_moved_document_statements();

COMMENT ON FUNCTION memory.detach_moved_document_statements() IS
    'A file item deleted because its document moved to a newer revision keeps its statements, detached, so the new revision can claim them. If the document is deleted or the item goes for another reason, the statements cascade away as before.';

CREATE FUNCTION memory.gone_deleted_document_statements() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    v_doc uuid := CASE TG_OP WHEN 'DELETE' THEN OLD.document_uuid ELSE NEW.document_uuid END;
    v_subjects bigint[];
BEGIN
    WITH g AS (
        UPDATE memory.knowledge_statements
           SET state = 'gone', resolved_by_statement_id = NULL
         WHERE document_uuid = v_doc AND item_id IS NULL AND state <> 'gone'
        RETURNING subject_id
    )
    SELECT array_agg(DISTINCT subject_id) INTO v_subjects FROM g WHERE subject_id IS NOT NULL;
    IF v_subjects IS NOT NULL THEN
        PERFORM memory.refresh_knowledge_subjects(v_subjects);
        PERFORM memory.mark_knowledge_subjects_changed(v_subjects, now());
    END IF;
    RETURN NULL;
END
$$;

CREATE TRIGGER trg_knowledge_documents_gone_statements AFTER UPDATE OF deleted_at ON memory.knowledge_documents
    FOR EACH ROW WHEN (NEW.deleted_at IS NOT NULL AND OLD.deleted_at IS NULL)
    EXECUTE FUNCTION memory.gone_deleted_document_statements();

CREATE TRIGGER trg_knowledge_documents_gone_statements_delete BEFORE DELETE ON memory.knowledge_documents
    FOR EACH ROW EXECUTE FUNCTION memory.gone_deleted_document_statements();

CREATE FUNCTION memory.rehome_document_statements(p_item_id bigint, p_body text, p_judge_after timestamptz)
RETURNS TABLE (outcome text, statement_id bigint, range_start integer, range_end integer)
LANGUAGE plpgsql AS $$
DECLARE
    v_doc uuid;
    v_rev bigint;
    v_bytes bytea := convert_to(p_body, 'UTF8');
    v_prev integer := 0;
    v_at integer;
    v_len integer;
    r record;
    v_ids bigint[] := ARRAY[]::bigint[];
    v_starts integer[] := ARRAY[]::integer[];
    v_ends integer[] := ARRAY[]::integer[];
    v_gone bigint[] := ARRAY[]::bigint[];
    v_subjects bigint[];
BEGIN
    SELECT r2.document_uuid, r2.id INTO v_doc, v_rev
      FROM memory.knowledge_items i
      JOIN memory.knowledge_revisions r2 ON r2.id = i.source_id
     WHERE i.id = p_item_id AND i.kind = 'artifact';
    IF v_doc IS NULL THEN
        RETURN;
    END IF;
    FOR r IN
        SELECT s.id,
               CASE WHEN s.passage_start IS NOT NULL AND prv.body IS NOT NULL AND s.passage_end > s.passage_start
                    THEN substring(convert_to(prv.body, 'UTF8') FROM s.passage_start + 1
                                   FOR s.passage_end - s.passage_start) END AS passage
          FROM memory.knowledge_statements s
          LEFT JOIN memory.knowledge_revisions prv ON prv.id = s.revision_id
         WHERE s.document_uuid = v_doc AND s.item_id IS NULL AND s.state <> 'gone'
         ORDER BY s.passage_start NULLS LAST, s.id
           FOR UPDATE OF s
    LOOP
        v_at := CASE WHEN r.passage IS NULL THEN 0
                     ELSE position(r.passage IN substring(v_bytes FROM v_prev + 1)) END;
        IF v_at = 0 THEN
            v_gone := v_gone || r.id;
        ELSE
            v_len := octet_length(r.passage);
            v_ids := v_ids || r.id;
            v_starts := v_starts || (v_prev + v_at - 1);
            v_ends := v_ends || (v_prev + v_at - 1 + v_len);
            v_prev := v_prev + v_at - 1 + v_len;
        END IF;
    END LOOP;
    UPDATE memory.knowledge_statements s
       SET item_id = p_item_id, detached_at = NULL, revision_id = v_rev,
           passage_start = m.s, passage_end = m.e,
           ordinal = (SELECT COALESCE(max(x.ordinal) + 1, 0) FROM memory.knowledge_statements x WHERE x.item_id = p_item_id) + m.n - 1
      FROM unnest(v_ids, v_starts, v_ends) WITH ORDINALITY m(id, s, e, n)
     WHERE s.id = m.id;
    WITH g AS (
        UPDATE memory.knowledge_statements s
           SET state = 'gone', resolved_by_statement_id = NULL
         WHERE s.id = ANY(v_gone)
        RETURNING s.subject_id
    )
    SELECT array_agg(DISTINCT g.subject_id) INTO v_subjects FROM g WHERE g.subject_id IS NOT NULL;
    IF v_subjects IS NOT NULL THEN
        PERFORM memory.refresh_knowledge_subjects(v_subjects);
        PERFORM memory.mark_knowledge_subjects_changed(v_subjects, p_judge_after);
    END IF;
    RETURN QUERY SELECT 'rehomed'::text, m.id, m.s, m.e FROM unnest(v_ids, v_starts, v_ends) m(id, s, e);
    RETURN QUERY SELECT 'gone'::text, g.id, NULL::integer, NULL::integer FROM unnest(v_gone) g(id);
    RETURN QUERY
        WITH iv AS (
            SELECT u.s, u.e, max(u.e) OVER (ORDER BY u.s, u.e ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING) AS prev_end
              FROM unnest(v_starts, v_ends) u(s, e)
        )
        SELECT 'uncovered'::text, NULL::bigint, COALESCE(iv.prev_end, 0), iv.s FROM iv WHERE iv.s > COALESCE(iv.prev_end, 0)
        UNION ALL
        SELECT 'uncovered'::text, NULL::bigint, COALESCE(max(iv.e), 0), octet_length(v_bytes) FROM iv
         HAVING COALESCE(max(iv.e), 0) < octet_length(v_bytes);
END
$$;

COMMENT ON FUNCTION memory.rehome_document_statements(bigint, text, timestamptz) IS
    'Gives the detached statements of the item''s document to the item (a new revision of the file). A statement whose passage text, read from its old revision, occurs in the new body (bytes, whitespace-exact, first occurrence at or after the previous match) keeps its id, subject, state, date, embedding and text and gets the new passage offsets. The others become gone and their subjects are marked changed. Returns outcome rehomed (statement_id, range), gone (statement_id) and uncovered (byte range of the body not covered by a rehomed passage).';

CREATE FUNCTION memory.sweep_detached_knowledge_statements(p_older_than interval, p_judge_after timestamptz) RETURNS bigint
LANGUAGE plpgsql AS $$
DECLARE
    v_subjects bigint[];
    v_count bigint;
BEGIN
    WITH g AS (
        UPDATE memory.knowledge_statements
           SET state = 'gone', resolved_by_statement_id = NULL
         WHERE item_id IS NULL AND state <> 'gone' AND detached_at < now() - p_older_than
        RETURNING subject_id
    )
    SELECT count(*), array_agg(DISTINCT subject_id) FILTER (WHERE subject_id IS NOT NULL) INTO v_count, v_subjects FROM g;
    IF v_subjects IS NOT NULL THEN
        PERFORM memory.refresh_knowledge_subjects(v_subjects);
        PERFORM memory.mark_knowledge_subjects_changed(v_subjects, p_judge_after);
    END IF;
    RETURN v_count;
END
$$;

COMMENT ON FUNCTION memory.sweep_detached_knowledge_statements(interval, timestamptz) IS
    'Marks detached statements gone once no new revision has claimed them for p_older_than, and marks their subjects changed.';

CREATE FUNCTION memory.requeue_knowledge_twin_heirs() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM old_rows o WHERE o.kind = 'artifact') THEN
        RETURN NULL;
    END IF;
    WITH t AS (
        SELECT tw.revision_id, tw.twin_item_id,
               EXISTS (SELECT 1 FROM memory.knowledge_statements s
                        WHERE s.item_id = tw.twin_item_id AND s.state = 'current') AS owns
          FROM memory.knowledge_artifact_twins(ARRAY(SELECT o.source_id FROM old_rows o WHERE o.kind = 'artifact')) tw
    ), heirs AS (
        SELECT min(t.twin_item_id) AS item_id
          FROM t
         GROUP BY t.revision_id
        HAVING NOT bool_or(t.owns)
    )
    UPDATE memory.knowledge_items i SET consolidate_at = clock_timestamp()
      FROM heirs h
     WHERE i.id = h.item_id;
    RETURN NULL;
END
$$;

CREATE TRIGGER trg_knowledge_items_requeue_twin_heirs AFTER DELETE ON memory.knowledge_items
    REFERENCING OLD TABLE AS old_rows FOR EACH STATEMENT
    EXECUTE FUNCTION memory.requeue_knowledge_twin_heirs();

COMMENT ON FUNCTION memory.requeue_knowledge_twin_heirs() IS
    'When a file item is deleted (its revision was replaced, or the file went), the surviving twin that becomes canonical is queued if no twin owns statements any more.';

INSERT INTO memory.consolidation_run_kinds (id, code) OVERRIDING SYSTEM VALUE VALUES (4, 'knowledge')
ON CONFLICT (id) DO NOTHING;
SELECT setval('memory.consolidation_run_kinds_id_seq', (SELECT max(id) FROM memory.consolidation_run_kinds));

ALTER TABLE memory.lessons DROP CONSTRAINT IF EXISTS lessons_retired_kind_check;
ALTER TABLE memory.lessons ADD CONSTRAINT lessons_retired_kind_check
    CHECK (retired_kind IS NULL OR retired_kind IN ('outcome', 'superseded', 'consolidated'));

COMMENT ON COLUMN memory.lessons.retired_kind IS
    'outcome: its supply verdicts fell below lesson_outcome.retire_floor over at least lesson_outcome.min_judged_supplies judged supplies. superseded: an adjudicated contradiction by a newer lesson. consolidated: knowledge consolidation found it covered, merged or contradicted by other statements; retired_by_lesson_id names the lesson that replaced it when there is one.';

CREATE TABLE IF NOT EXISTS memory.knowledge_decision_verdicts (
    code text PRIMARY KEY CHECK (code ~ '^[a-z][a-z_]{0,29}$')
);

INSERT INTO memory.knowledge_decision_verdicts (code) VALUES
    ('kept'), ('covered'), ('superseded'), ('merged'), ('tightened'), ('corrected'), ('misgrouped'), ('conflict_open')
ON CONFLICT (code) DO NOTHING;

COMMENT ON TABLE memory.knowledge_decision_verdicts IS
    'What a judgement concluded about a statement: kept as it is, covered by another, superseded by another, merged into another, tightened (rewritten shorter), corrected (rewritten to say what the operator''s later words now ask for), misgrouped (does not belong in its subject), conflict_open (contradicts another statement and no precedence rule settles it).';

CREATE TABLE IF NOT EXISTS memory.knowledge_decision_applications (
    code text PRIMARY KEY CHECK (code ~ '^[a-z][a-z_]{0,29}$')
);

INSERT INTO memory.knowledge_decision_applications (code) VALUES
    ('applied'), ('proposed'), ('failed'), ('undone'), ('none')
ON CONFLICT (code) DO NOTHING;

COMMENT ON TABLE memory.knowledge_decision_applications IS
    'What became of a decision on its carrier: applied, proposed (the exact edit is shown, nothing written), failed, undone from the stored earlier text, or none (nothing to apply).';

CREATE TABLE IF NOT EXISTS memory.knowledge_consolidation_decisions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    run_id bigint NOT NULL REFERENCES memory.consolidation_runs (id) ON DELETE CASCADE,
    project_id bigint REFERENCES projects.projects (id) ON DELETE CASCADE,
    subject_id bigint REFERENCES memory.knowledge_subjects (id) ON DELETE SET NULL,
    statement_id bigint REFERENCES memory.knowledge_statements (id) ON DELETE SET NULL,
    item_id bigint REFERENCES memory.knowledge_items (id) ON DELETE SET NULL,
    carrier_reference text NOT NULL,
    verdict text NOT NULL REFERENCES memory.knowledge_decision_verdicts (code),
    counterpart_statement_id bigint REFERENCES memory.knowledge_statements (id) ON DELETE SET NULL,
    counterpart_reference text,
    text_before text NOT NULL,
    text_after text,
    reason text NOT NULL,
    evidence jsonb NOT NULL DEFAULT '[]'::jsonb,
    model text NOT NULL,
    application text NOT NULL DEFAULT 'none' REFERENCES memory.knowledge_decision_applications (code),
    applied_at timestamptz,
    error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT knowledge_decisions_failed_shape CHECK ((application = 'failed') = (error IS NOT NULL)),
    CONSTRAINT knowledge_decisions_applied_shape CHECK (application <> 'applied' OR applied_at IS NOT NULL)
);

COMMENT ON TABLE memory.knowledge_consolidation_decisions IS
    'One row per judged statement: the verdict, the counterpart, the text before and after, the reason, the evidence read, the model and whether it was applied. Survives the statement, item and subject it names (their ids become NULL; carrier_reference and the texts remain). evidence is a list of the chunks read, not a filter or join target.';

CREATE INDEX IF NOT EXISTS idx_knowledge_decisions_project_created
    ON memory.knowledge_consolidation_decisions (project_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_knowledge_decisions_subject_created
    ON memory.knowledge_consolidation_decisions (subject_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_knowledge_decisions_run ON memory.knowledge_consolidation_decisions (run_id);
CREATE INDEX IF NOT EXISTS idx_knowledge_decisions_statement
    ON memory.knowledge_consolidation_decisions (statement_id) WHERE statement_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_knowledge_decisions_counterpart
    ON memory.knowledge_consolidation_decisions (counterpart_statement_id) WHERE counterpart_statement_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_knowledge_decisions_item
    ON memory.knowledge_consolidation_decisions (item_id) WHERE item_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS memory.knowledge_consolidation_measures (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    run_id bigint NOT NULL REFERENCES memory.consolidation_runs (id) ON DELETE CASCADE,
    project_id bigint REFERENCES projects.projects (id) ON DELETE CASCADE,
    measured_at timestamptz NOT NULL DEFAULT now(),
    statements_current integer NOT NULL CHECK (statements_current >= 0),
    subjects integer NOT NULL CHECK (subjects >= 0),
    subjects_shared integer NOT NULL CHECK (subjects_shared >= 0),
    conflicts_open integer NOT NULL CHECK (conflicts_open >= 0),
    guidance_chars bigint NOT NULL CHECK (guidance_chars >= 0),
    misgrouped_share real NOT NULL CHECK (misgrouped_share BETWEEN 0 AND 1),
    split_subject_share real NOT NULL CHECK (split_subject_share BETWEEN 0 AND 1),
    grouping_distance real NOT NULL CHECK (grouping_distance BETWEEN 0 AND 2)
);

COMMENT ON TABLE memory.knowledge_consolidation_measures IS
    'One row per project (NULL: machine-wide) per run: statements in force, subjects, subjects held by more than one carrier, open conflicts, characters of current statement text, the two error shares that steer the grouping distance and the distance in effect.';

CREATE INDEX IF NOT EXISTS idx_knowledge_measures_project_measured
    ON memory.knowledge_consolidation_measures (project_id, measured_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_knowledge_measures_run ON memory.knowledge_consolidation_measures (run_id);
