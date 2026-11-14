INSERT INTO workspace.code_findings
       (scan_id, change_event_id, checkout, path, rule_id, severity, message, line, col,
        matched, fix, fingerprint)
SELECT $1, $2, $3, $4, f.rule_id, f.severity, f.message, f.line, f.col, f.matched, f.fix, f.fingerprint
  FROM unnest($5::text[], $6::text[], $7::text[], $8::int[], $9::int[], $10::text[], $11::text[], $12::text[])
       AS f(rule_id, severity, message, line, col, matched, fix, fingerprint)
