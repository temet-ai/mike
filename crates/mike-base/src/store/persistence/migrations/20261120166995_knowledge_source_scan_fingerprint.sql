-- The knowledge scan remembers, per source, the fingerprint of the document set
-- it last ingested and the generation that ingest produced. After a restart the
-- scan compares against this instead of ingesting every source again; an ingest
-- by anyone else moves the generation, so a stale fingerprint never matches.
ALTER TABLE memory.knowledge_sources
    ADD COLUMN scan_fingerprint text,
    ADD COLUMN scan_fingerprint_generation bigint;
