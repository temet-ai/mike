INSERT INTO workspace.code_findings
       (scan_id, change_event_id, checkout, path, rule_id, rule_source, severity, message, line, col,
        matched, fix, fingerprint)
SELECT $1, $2, $3, $4, f.rule_id, f.rule_source, f.severity, f.message, f.line, f.col, f.matched, f.fix, f.fingerprint
  FROM unnest($5::text[], $6::text[], $7::text[], $8::text[], $9::int[], $10::int[], $11::text[], $12::text[], $13::text[])
       AS f(rule_id, rule_source, severity, message, line, col, matched, fix, fingerprint)
