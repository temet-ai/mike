-- Session overviews become searchable project knowledge. The session judge writes about 500 characters for every
-- finished session into sessions.sessions.overview; search could not return them. The new view
-- memory.knowledge_sources_with_sessions_v lists every row of memory.knowledge_embedding_sources_v plus each session
-- of a project that has a non-empty overview as source kind 'session', versioned by overview_generated_at (NULL for an
-- overview an operator wrote by hand, which is then embedded once and not again). The two embedding state constraints
-- accept the new kind, with source_updated_at allowed for it.
-- memory.knowledge_embedding_sources_v is left unchanged: binaries that do not know the 'session' kind read it.
SET LOCAL statement_timeout = 0;
SET LOCAL lock_timeout = '10s';

ALTER TABLE memory.knowledge_embedding_state
    DROP CONSTRAINT knowledge_embedding_state_source_kind_check,
    DROP CONSTRAINT knowledge_embedding_state_version_shape_check;

ALTER TABLE memory.knowledge_embedding_state
    ADD CONSTRAINT knowledge_embedding_state_source_kind_check
        CHECK (source_kind IN ('artifact', 'lesson', 'note', 'thread', 'thread_entry',
                               'question_answer', 'unmet_ask', 'correction', 'learning', 'mail',
                               'session'))
        NOT VALID,
    ADD CONSTRAINT knowledge_embedding_state_version_shape_check
        CHECK ((source_kind IN ('note', 'question_answer', 'learning', 'session')
                OR source_updated_at IS NULL)
           AND (source_kind = 'thread' OR source_seq IS NULL)
           AND (source_kind = 'lesson' OR source_hash IS NULL))
        NOT VALID;

ALTER TABLE memory.knowledge_embedding_state
    VALIDATE CONSTRAINT knowledge_embedding_state_source_kind_check;
ALTER TABLE memory.knowledge_embedding_state
    VALIDATE CONSTRAINT knowledge_embedding_state_version_shape_check;

CREATE VIEW memory.knowledge_sources_with_sessions_v AS
SELECT * FROM memory.knowledge_embedding_sources_v
UNION ALL
SELECT 'session'::text, s.id, 'session:' || s.session_uuid, s.project_id, s.overview_generated_at,
       NULL::bigint, NULL::text
  FROM sessions.sessions s
 WHERE btrim(coalesce(s.overview, '')) <> '' AND s.project_id IS NOT NULL;

COMMENT ON VIEW memory.knowledge_sources_with_sessions_v IS
    'Every row of memory.knowledge_embedding_sources_v plus one row per session of a project with a non-blank overview (source kind session, source id the session id, version overview_generated_at). Read by the builds that know the session kind; the first view stays as it was for the builds that do not.';
