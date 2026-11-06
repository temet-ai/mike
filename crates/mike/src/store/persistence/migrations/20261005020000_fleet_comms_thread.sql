-- Every fleet gets one comms thread, and mail its seats exchange is filed there.
--
-- Before this, 3,087 mails between fleet seats had no thread, so /comms never
-- showed them and project knowledge could not search them. A session already
-- knows its project, and a seat's launch row names the fleet run it belongs
-- to, so the fleet a mail belongs to is derived from its sender, or, when
-- the sender is not a seat, its recipient (mike's own mail, and a session
-- outside the fleet, such as the one that started it, relaying into it).
--
-- The thread's uuid is the fleet's uuid, so a brief can name it without
-- another lookup.
--
-- The table rewrites come first, while this transaction holds no row locks,
-- so a live mail insert cannot wait on us while we wait on it.

LOCK TABLE workspace.session_notes, mailbox.messages, mailbox.threads IN ACCESS EXCLUSIVE MODE;

-- Search stored once per row instead of rebuilt on every query: session
-- notes were 159 ms of a 184 ms project search, and thread search ranks
-- message bodies the same way.
ALTER TABLE mailbox.messages ADD COLUMN search_tsv tsvector
    GENERATED ALWAYS AS (to_tsvector('english', body)) STORED;
ALTER TABLE workspace.session_notes ADD COLUMN search_tsv tsvector
    GENERATED ALWAYS AS (to_tsvector('english', title || ' ' || body)) STORED;
CREATE INDEX messages_search_tsv ON mailbox.messages USING gin (search_tsv) WHERE thread_id IS NOT NULL;
CREATE INDEX session_notes_search_tsv ON workspace.session_notes USING gin (search_tsv);

ALTER TABLE mailbox.threads ADD COLUMN fleet_uuid uuid;
CREATE UNIQUE INDEX threads_fleet_uuid ON mailbox.threads(fleet_uuid) WHERE fleet_uuid IS NOT NULL;
COMMENT ON COLUMN mailbox.threads.fleet_uuid IS
    'Set on a fleet''s comms thread (one per fleet; thread_uuid = fleet_uuid). Mail to or from that fleet''s seats with no thread of its own is filed here by mailbox.file_fleet_mail().';

CREATE FUNCTION mailbox.session_fleet_uuid(p_session_id bigint) RETURNS uuid
LANGUAGE sql STABLE AS $$
    SELECT f.fleet_uuid
      FROM sessions.sessions s
      JOIN launch.launches l ON l.session_uuid = s.session_uuid
      JOIN launch.fleets f ON f.run_id = l.run_id
     WHERE s.id = p_session_id
     ORDER BY l.created_at DESC, l.id DESC
     LIMIT 1
$$;
COMMENT ON FUNCTION mailbox.session_fleet_uuid(bigint) IS
    'The fleet a session was launched into as a seat (its newest launch), or NULL.';

CREATE FUNCTION mailbox.fleet_project_id(p_fleet_uuid uuid) RETURNS bigint
LANGUAGE sql STABLE AS $$
    SELECT COALESCE(
        (SELECT COALESCE(p.merged_into_id, p.id)
           FROM launch.fleets f
           JOIN projects.projects p ON p.project_path = f.project_path
          WHERE f.fleet_uuid = p_fleet_uuid
          LIMIT 1),
        (SELECT s.project_id
           FROM launch.fleets f
           JOIN launch.launches l ON l.run_id = f.run_id
           JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
          WHERE f.fleet_uuid = p_fleet_uuid AND s.project_id IS NOT NULL
          ORDER BY l.created_at
          LIMIT 1))
$$;

CREATE FUNCTION mailbox.ensure_fleet_thread(p_fleet_uuid uuid) RETURNS uuid
LANGUAGE sql AS $$
    INSERT INTO mailbox.threads (thread_uuid, subject, project_id, fleet_uuid, created_at, last_message_at)
    SELECT f.fleet_uuid,
           'Fleet ' || f.slug || COALESCE(': ' || NULLIF(btrim(f.definition ->> 'goal'), ''), ''),
           mailbox.fleet_project_id(f.fleet_uuid),
           f.fleet_uuid,
           COALESCE(f.started_at, f.ready_at, f.created_at),
           COALESCE(f.started_at, f.ready_at, f.created_at)
      FROM launch.fleets f
     WHERE f.fleet_uuid = p_fleet_uuid
    ON CONFLICT (fleet_uuid) WHERE fleet_uuid IS NOT NULL
    DO UPDATE SET project_id = COALESCE(mailbox.threads.project_id, EXCLUDED.project_id)
    RETURNING thread_uuid
$$;
COMMENT ON FUNCTION mailbox.ensure_fleet_thread(uuid) IS
    'Creates the fleet''s comms thread if it does not exist and returns its uuid. Called when a fleet starts.';

CREATE FUNCTION mailbox.file_fleet_mail() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    v_fleet uuid;
    v_thread bigint;
BEGIN
    IF NEW.thread_id IS NOT NULL THEN
        RETURN NEW;
    END IF;
    IF EXISTS (SELECT 1 FROM launch.launches WHERE brief_note_uuid = NEW.message_uuid) THEN
        RETURN NEW;
    END IF;
    v_fleet := mailbox.session_fleet_uuid(NEW.from_session_id);
    IF v_fleet IS NULL THEN
        v_fleet := mailbox.session_fleet_uuid(NEW.to_session_id);
    END IF;
    IF v_fleet IS NULL THEN
        RETURN NEW;
    END IF;
    SELECT id INTO v_thread FROM mailbox.threads WHERE fleet_uuid = v_fleet;
    IF v_thread IS NULL THEN
        RETURN NEW;
    END IF;
    NEW.thread_id := v_thread;
    UPDATE mailbox.threads SET last_message_at = now() WHERE id = v_thread;
    RETURN NEW;
END
$$;

CREATE TRIGGER messages_file_fleet_mail
    BEFORE INSERT ON mailbox.messages
    FOR EACH ROW EXECUTE FUNCTION mailbox.file_fleet_mail();

CREATE INDEX IF NOT EXISTS launches_brief_note_uuid ON launch.launches(brief_note_uuid)
    WHERE brief_note_uuid IS NOT NULL;

-- Backfill: a thread for every fleet that has run, and its seats' unthreaded mail.
SELECT mailbox.ensure_fleet_thread(f.fleet_uuid) FROM launch.fleets f WHERE f.run_id IS NOT NULL;

WITH seat_fleet AS (
    SELECT DISTINCT ON (s.id) s.id AS session_id, f.fleet_uuid
      FROM sessions.sessions s
      JOIN launch.launches l ON l.session_uuid = s.session_uuid
      JOIN launch.fleets f ON f.run_id = l.run_id
     ORDER BY s.id, l.created_at DESC, l.id DESC
), filed AS (
    SELECT m.id, t.id AS thread_id
      FROM mailbox.messages m
      LEFT JOIN seat_fleet sender ON sender.session_id = m.from_session_id
      LEFT JOIN seat_fleet recipient ON recipient.session_id = m.to_session_id
      JOIN mailbox.threads t ON t.fleet_uuid = COALESCE(sender.fleet_uuid, recipient.fleet_uuid)
     WHERE m.thread_id IS NULL
       AND NOT EXISTS (SELECT 1 FROM launch.launches l WHERE l.brief_note_uuid = m.message_uuid)
)
UPDATE mailbox.messages m SET thread_id = filed.thread_id FROM filed WHERE m.id = filed.id;

UPDATE mailbox.threads t
   SET last_message_at = GREATEST(t.last_message_at, last.queued_at)
  FROM (SELECT thread_id, max(queued_at) AS queued_at FROM mailbox.messages GROUP BY thread_id) last
 WHERE t.fleet_uuid IS NOT NULL AND last.thread_id = t.id;

-- Threads become project knowledge: searchable, readable, and embedded once a
-- thread has been quiet for ten minutes (so a busy thread is not re-embedded
-- on every message).

CREATE VIEW mailbox.thread_text_v AS
SELECT t.id AS thread_id, t.thread_uuid, t.project_id, t.subject, t.last_message_at,
       t.resolved_at, t.fleet_uuid,
       t.subject || E'\n\n' || COALESCE(string_agg(
           COALESCE(NULLIF(btrim(sg.display_name), ''), left(s.session_uuid::text, 8), 'mike')
           || ': ' || m.body, E'\n\n' ORDER BY m.queued_at, m.id), '') AS body
  FROM mailbox.threads t
  LEFT JOIN mailbox.messages m ON m.thread_id = t.id
  LEFT JOIN sessions.sessions s ON s.id = m.from_session_id
  LEFT JOIN sessions.session_goals sg ON sg.session_id = s.id
 GROUP BY t.id, t.thread_uuid, t.project_id, t.subject, t.last_message_at, t.resolved_at, t.fleet_uuid;
COMMENT ON VIEW mailbox.thread_text_v IS
    'Each thread as one text: subject, then each message as "<sender name>: <body>" in order. Used by project knowledge.';

CREATE OR REPLACE VIEW memory.knowledge_embedding_sources_v AS
SELECT 'artifact:' || r.id AS reference, loc.project_id, r.content_hash AS content_hash,
       r.title || E'\n\n' || r.body AS body
FROM memory.knowledge_documents d
JOIN memory.knowledge_revisions r ON r.id = d.current_revision
JOIN memory.knowledge_sources src ON src.source_uuid = d.source_uuid
JOIN projects.locations loc ON loc.location_uuid = src.location_uuid
WHERE d.deleted_at IS NULL
UNION ALL
SELECT 'lesson:' || l.uuid, l.project_id, md5(l.text), l.text
FROM memory.lessons l WHERE l.project_id IS NOT NULL
UNION ALL
SELECT 'note:' || n.note_uuid, s.project_id, md5(n.title || E'\n\n' || n.body),
       n.title || E'\n\n' || n.body
FROM workspace.session_notes n JOIN sessions.sessions s ON s.id = n.session_id
UNION ALL
SELECT 'thread:' || tx.thread_uuid, tx.project_id, md5(tx.body), tx.body
FROM mailbox.thread_text_v tx
WHERE tx.project_id IS NOT NULL AND tx.last_message_at < now() - interval '10 minutes';
