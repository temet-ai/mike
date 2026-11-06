-- Collector-reported source health (#292, WP03), plus a durable record of lesson-file
-- conflicts so the health read can count them.
--
-- knowledge_sources gains what the collector reports about itself. Every new column is
-- nullable because "the collector has not reported yet" is a real state that must read as
-- unknown, never as healthy or failed; the server never derives a cause from an absent
-- success (standing decision 4).
--
--   last_heartbeat_at        the last time the collector reported at all (a heartbeat, or a
--                            failure report, which also proves the collector is running).
--                            It is a report time, not an online/offline verdict.
--   last_failure_at          when the collector last reported an aborted scan.
--   last_failure             the collector's full error text for that failure, never truncated.
--   last_failure_generation  the source's generation when the failure was recorded, taken from
--                            the row by the server rather than trusted from the caller. A
--                            failure whose generation is below the current generation predates
--                            a later successful scan.
--
-- The three failure columns are all set or all null, so a half-written failure cannot exist.
-- A successful scan deliberately leaves the last failure in place: it is history, and the
-- generation comparison says whether it is still current.

ALTER TABLE memory.knowledge_sources
    ADD COLUMN IF NOT EXISTS last_heartbeat_at timestamptz,
    ADD COLUMN IF NOT EXISTS last_failure_at timestamptz,
    ADD COLUMN IF NOT EXISTS last_failure text,
    ADD COLUMN IF NOT EXISTS last_failure_generation bigint;

ALTER TABLE memory.knowledge_sources
    DROP CONSTRAINT IF EXISTS knowledge_sources_failure_report;

ALTER TABLE memory.knowledge_sources
    ADD CONSTRAINT knowledge_sources_failure_report CHECK (
        (last_failure IS NULL
            AND last_failure_at IS NULL
            AND last_failure_generation IS NULL)
        OR (last_failure IS NOT NULL
            AND btrim(last_failure) <> ''
            AND last_failure_at IS NOT NULL
            AND last_failure_generation IS NOT NULL
            AND last_failure_generation >= 0)
    );

COMMENT ON COLUMN memory.knowledge_sources.last_heartbeat_at IS
    'When the collector last reported for this source (heartbeat or failure). NULL until the first report. A report time only: never evidence that a machine is online or offline.';
COMMENT ON COLUMN memory.knowledge_sources.last_failure_at IS
    'When the collector last reported an aborted scan. NULL when it has never reported one.';
COMMENT ON COLUMN memory.knowledge_sources.last_failure IS
    'The collector''s full error text for its last aborted scan, stored whole. Set together with last_failure_at and last_failure_generation.';
COMMENT ON COLUMN memory.knowledge_sources.last_failure_generation IS
    'The source generation at the moment the failure was recorded, read from the row by the server. Below generation means a later scan succeeded.';

-- Lesson-file conflicts. lesson_file_changes and lesson_file_receipts record what the
-- server has and what each location has been given, but a conflict (the checkout's file was
-- edited locally while the server copy moved on, so the collector preserved the file and
-- wrote nothing) is only ever known to the collector. Without a stored record the health
-- read could only ever report zero conflicts, which is a lie of omission.
--
-- One row per (location, lesson) that currently conflicts. server_version is the server's
-- lesson_file_changes.version when the collector reported it. A row is removed when the
-- location's receipt for that lesson is acknowledged, which is the collector saying the
-- file and the server agree again.

CREATE TABLE IF NOT EXISTS memory.lesson_file_conflicts (
    location_uuid  uuid        NOT NULL
                               REFERENCES projects.locations(location_uuid) ON DELETE CASCADE,
    lesson_uuid    uuid        NOT NULL
                               REFERENCES memory.lesson_file_changes(lesson_uuid) ON DELETE CASCADE,
    server_version bigint      NOT NULL CHECK (server_version >= 1),
    detected_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (location_uuid, lesson_uuid)
);

-- The primary key leads with location_uuid and serves that foreign key; this serves the
-- lesson_uuid one.
CREATE INDEX IF NOT EXISTS lesson_file_conflicts_lesson
    ON memory.lesson_file_conflicts (lesson_uuid);

COMMENT ON TABLE memory.lesson_file_conflicts IS
    'Lesson files a collector reported as conflicting at a location: edited locally while the server copy moved on, so the file was preserved. Cleared when the location acknowledges a receipt for the lesson. Read by the project knowledge health endpoint.';
