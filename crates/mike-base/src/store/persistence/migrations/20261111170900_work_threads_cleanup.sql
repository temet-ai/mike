DO $$
BEGIN
    IF to_regclass('steering.thread_acceptance_criteria') IS NOT NULL THEN
        INSERT INTO work.thread_acceptance_criteria
               (thread_id, criteria_text, source_path, parsed_at, parse_error)
        SELECT t.id, s.criteria_text, s.source_path, s.parsed_at, s.parse_error
          FROM steering.thread_acceptance_criteria s
          JOIN work.threads t ON t.slug = s.thread_slug
         WHERE NOT EXISTS (
                   SELECT 1 FROM work.thread_acceptance_criteria c WHERE c.thread_id = t.id)
           AND NOT EXISTS (
                   SELECT 1 FROM work.threads o WHERE o.slug = s.thread_slug AND o.id <> t.id);
        DROP TABLE steering.thread_acceptance_criteria;
    END IF;
END
$$;

UPDATE work.threads
   SET title = NULLIF(btrim(substr(btrim(title), 8)), '')
 WHERE btrim(title) ILIKE 'thread:%'
   AND (NULLIF(btrim(substr(btrim(title), 8)), '') IS NOT NULL OR parse_error IS NOT NULL);

UPDATE work.threads
   SET title = slug
 WHERE btrim(title) ILIKE 'thread:%';

UPDATE work.threads
   SET status = CASE status WHEN 'complete' THEN 'done' ELSE 'implementing' END
 WHERE status IN ('complete', 'in-progress');

ALTER TABLE work.threads DROP CONSTRAINT IF EXISTS threads_status_check;
ALTER TABLE work.threads
    ADD CONSTRAINT threads_status_check
    CHECK (status IN ('active', 'implementing', 'review', 'parked', 'done'));
