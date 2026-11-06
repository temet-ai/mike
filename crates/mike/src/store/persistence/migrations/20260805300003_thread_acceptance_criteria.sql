-- Issue #119: bridge thread-worklog acceptance criteria into steering so the
-- delivery-match (false_done) judge can check a threaded session against its
-- real, operator-approved criteria instead of a reconstructed ask.
--
-- One row per thread (re-parsed and upserted on demand, at judge time, from
-- `artifacts/threads/<thread_slug>/worklog.md` — see
-- `steerer::thread_criteria`). `parse_error` is set (never a silently
-- dropped row) when a worklog exists but its `<acceptance-criteria>` block
-- couldn't be extracted, so a parser failure stays visible per AC4 instead
-- of looking identical to "no worklog for this thread".
CREATE TABLE steering.thread_acceptance_criteria (
    thread_slug   text PRIMARY KEY,
    criteria_text text NULL,
    source_path   text NOT NULL,
    parsed_at     timestamptz NOT NULL DEFAULT now(),
    parse_error   text NULL,
    CONSTRAINT thread_acceptance_criteria_has_text_or_error
        CHECK (criteria_text IS NOT NULL OR parse_error IS NOT NULL)
);

COMMENT ON TABLE steering.thread_acceptance_criteria IS
    'Parsed <acceptance-criteria> blocks from artifacts/threads/*/worklog.md, keyed by thread slug — issue #119.';
COMMENT ON COLUMN steering.thread_acceptance_criteria.parse_error IS
    'Non-NULL when the worklog was read but no acceptance-criteria block could be extracted; the row still exists so the failure is visible, not swallowed.';
