INSERT INTO workspace.code_findings
       (scan_id, change_event_id, checkout, path, rule_id, rule_source, severity, message, line, col,
        matched, fix, fingerprint, outcome, outcome_at, answer, answer_reason, answer_ruling, answered_at)
SELECT $1, $2, $3, $4, f.rule_id, f.rule_source, f.severity, f.message, f.line, f.col, f.matched, f.fix, f.fingerprint,
       CASE WHEN f.kept IS NULL THEN 'open' ELSE 'kept' END,
       CASE WHEN f.kept IS NULL THEN NULL ELSE now() END,
       CASE WHEN f.kept IS NULL THEN NULL ELSE 'kept' END,
       f.kept,
       CASE WHEN f.kept IS NULL THEN NULL ELSE 'Kept by the reason written beside the code.' END,
       CASE WHEN f.kept IS NULL THEN NULL ELSE now() END
  FROM unnest($5::text[], $6::text[], $7::text[], $8::text[], $9::int[], $10::int[], $11::text[], $12::text[], $13::text[], $14::text[])
       AS f(rule_id, rule_source, severity, message, line, col, matched, fix, fingerprint, kept)
