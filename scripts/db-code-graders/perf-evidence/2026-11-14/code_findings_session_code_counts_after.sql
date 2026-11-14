WITH ev AS (
    SELECT e.id
      FROM workspace.change_events e
      JOIN sessions.sessions ss ON ss.id = e.session_id
     WHERE ss.session_uuid = 'e0417bd0-4ee5-48c5-afb5-e544ac48df7e'::uuid
       AND e.kind IN ('edit', 'shell_change')
)
SELECT (SELECT count(*)
          FROM workspace.code_scans sc
          JOIN ev ON ev.id = sc.change_event_id
         WHERE sc.skipped IS NULL AND sc.error IS NULL) AS "files_scanned!",
       count(f.id) AS "found!",
       count(f.id) FILTER (WHERE f.delivered_at IS NOT NULL) AS "told!",
       count(f.id) FILTER (WHERE f.outcome = 'fixed' AND f.delivered_at IS NOT NULL
                             AND f.outcome_at > f.delivered_at) AS "fixed_after_told!",
       count(f.id) FILTER (WHERE f.outcome = 'fixed') AS "fixed!",
       count(f.id) FILTER (WHERE f.outcome = 'ignored') AS "ignored!",
       count(f.id) FILTER (WHERE f.outcome = 'open') AS "open!"
  FROM workspace.code_findings f
  JOIN ev ON ev.id = f.change_event_id
