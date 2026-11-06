INSERT INTO steering.thread_acceptance_criteria
     (thread_slug, criteria_text, source_path, parsed_at, parse_error)
 VALUES ('steerer-firing-model', 'criteria text', 'artifacts/threads/steerer-firing-model/worklog.md', NOW(), NULL)
 ON CONFLICT (thread_slug) DO UPDATE
     SET criteria_text = EXCLUDED.criteria_text,
         source_path   = EXCLUDED.source_path,
         parsed_at     = EXCLUDED.parsed_at,
         parse_error   = EXCLUDED.parse_error;
