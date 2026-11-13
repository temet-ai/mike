-- Every comms thread names a real project, and every tag is a word.
--
-- 92 threads sat under the placeholder project "(no project)" or under none,
-- because a thread took its recipient's project and some sessions never had
-- theirs resolved. The sessions that wrote in a thread nearly always know the
-- project between them, and the rest can be placed from their working folder.
--
-- Tags were free text: 15 were bare issue numbers ("152") and two pairs
-- differed only in case ("AC2", "ac2"). A tag is now lower case, a bare
-- number becomes "issue-<n>", and anything with no letter is not a tag.

CREATE FUNCTION mailbox.unknown_project_id() RETURNS bigint
LANGUAGE sql STABLE AS $$
    SELECT id FROM projects.projects WHERE project_path = '(no project)'
$$;

CREATE FUNCTION mailbox.session_real_project_id(p_session_id bigint) RETURNS bigint
LANGUAGE plpgsql STABLE AS $$
DECLARE
    v_project bigint;
    v_cwd text;
    v_base text;
BEGIN
    SELECT COALESCE(p.merged_into_id, p.id), btrim(s.cwd, '/')
      INTO v_project, v_cwd
      FROM sessions.sessions s
      LEFT JOIN projects.projects p ON p.id = s.project_id
     WHERE s.id = p_session_id;
    IF v_project IS NOT NULL AND v_project IS DISTINCT FROM mailbox.unknown_project_id() THEN
        RETURN v_project;
    END IF;
    IF v_cwd IS NULL OR v_cwd = '' THEN
        RETURN NULL;
    END IF;
    SELECT COALESCE(p.merged_into_id, p.id) INTO v_project
      FROM (
          SELECT id AS project_id, btrim(project_path, '/') AS root FROM projects.projects
          UNION ALL
          SELECT project_id, btrim(root_path, '/') FROM projects.locations
      ) r
      JOIN projects.projects p ON p.id = r.project_id
     WHERE p.project_path <> '(no project)'
       AND r.root <> ''
       AND (v_cwd = r.root OR v_cwd LIKE r.root || '/%')
     ORDER BY length(r.root) DESC
     LIMIT 1;
    IF v_project IS NOT NULL THEN
        RETURN v_project;
    END IF;
    v_base := regexp_replace(v_cwd, '^.*/', '');
    SELECT min(COALESCE(p.merged_into_id, p.id)) INTO v_project
      FROM projects.projects p
     WHERE regexp_replace(btrim(p.project_path, '/'), '^.*/', '') = v_base
       AND p.project_path <> '(no project)'
    HAVING count(DISTINCT COALESCE(p.merged_into_id, p.id)) = 1;
    RETURN v_project;
END
$$;
COMMENT ON FUNCTION mailbox.session_real_project_id(bigint) IS
    'The session''s project, or when it has none or only the "(no project)" placeholder, the project whose folder (or one of its locations) holds the session''s working folder, else the one project with the same folder name.';

CREATE FUNCTION mailbox.thread_project_id(p_thread_id bigint) RETURNS bigint
LANGUAGE sql STABLE AS $$
    WITH members AS (
        SELECT created_by_session_id AS session_id FROM mailbox.threads WHERE id = p_thread_id
        UNION ALL
        SELECT from_session_id FROM mailbox.messages WHERE thread_id = p_thread_id
        UNION ALL
        SELECT to_session_id FROM mailbox.messages WHERE thread_id = p_thread_id
    ), weighted AS (
        SELECT session_id, count(*) AS weight FROM members WHERE session_id IS NOT NULL GROUP BY session_id
    )
    SELECT r.project_id
      FROM weighted s
      CROSS JOIN LATERAL (SELECT mailbox.session_real_project_id(s.session_id) AS project_id) r
     WHERE r.project_id IS NOT NULL
     GROUP BY r.project_id
     ORDER BY sum(s.weight) DESC, r.project_id
     LIMIT 1
$$;
COMMENT ON FUNCTION mailbox.thread_project_id(bigint) IS
    'The project most of a thread''s mail comes from or goes to, by session_real_project_id. NULL when no participant can be placed.';

CREATE FUNCTION mailbox.place_thread_in_project() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    v_current bigint;
    v_project bigint;
BEGIN
    SELECT project_id INTO v_current FROM mailbox.threads WHERE id = NEW.thread_id;
    IF v_current IS NOT NULL AND v_current IS DISTINCT FROM mailbox.unknown_project_id() THEN
        RETURN NULL;
    END IF;
    v_project := mailbox.thread_project_id(NEW.thread_id);
    IF v_project IS NOT NULL THEN
        UPDATE mailbox.threads SET project_id = v_project WHERE id = NEW.thread_id;
    END IF;
    RETURN NULL;
END
$$;

CREATE TRIGGER messages_place_thread_in_project
    AFTER INSERT OR UPDATE OF thread_id ON mailbox.messages
    FOR EACH ROW WHEN (NEW.thread_id IS NOT NULL)
    EXECUTE FUNCTION mailbox.place_thread_in_project();

UPDATE mailbox.threads t
   SET project_id = placed.project_id
  FROM (
      SELECT id, mailbox.thread_project_id(id) AS project_id
        FROM mailbox.threads
       WHERE project_id IS NULL OR project_id = mailbox.unknown_project_id()
  ) placed
 WHERE placed.id = t.id AND placed.project_id IS NOT NULL;

CREATE FUNCTION mailbox.normalize_tag(p_code text) RETURNS text
LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE
               WHEN c ~ '^[0-9]+$' THEN 'issue-' || c
               WHEN c ~ '[a-z]' THEN c
           END
      FROM (SELECT regexp_replace(regexp_replace(lower(btrim(p_code)), '\s+', '-', 'g'), '^#+', '') AS c) n
$$;
COMMENT ON FUNCTION mailbox.normalize_tag(text) IS
    'A tag as stored: lower case, spaces as hyphens, a leading # dropped, a bare number as issue-<n>. NULL when what is left has no letter.';

CREATE TEMP TABLE tag_moves ON COMMIT DROP AS
SELECT id AS old_id, mailbox.normalize_tag(code) AS new_code
  FROM mailbox.tags
 WHERE mailbox.normalize_tag(code) IS DISTINCT FROM code;

INSERT INTO mailbox.tags (code)
SELECT DISTINCT new_code FROM tag_moves WHERE new_code IS NOT NULL
ON CONFLICT (code) DO NOTHING;

INSERT INTO mailbox.thread_tags (thread_id, tag_id)
SELECT tt.thread_id, t.id
  FROM tag_moves m
  JOIN mailbox.thread_tags tt ON tt.tag_id = m.old_id
  JOIN mailbox.tags t ON t.code = m.new_code
ON CONFLICT DO NOTHING;

DELETE FROM mailbox.tags t USING tag_moves m WHERE t.id = m.old_id;

ALTER TABLE mailbox.tags ADD CONSTRAINT tags_code_normalized
    CHECK (code = mailbox.normalize_tag(code));
