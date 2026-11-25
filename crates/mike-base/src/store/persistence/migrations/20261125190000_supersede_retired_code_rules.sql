-- These rules left the built-in set: they are one project's style and now come
-- only from that project's own rule files. Findings the built-in set raised
-- before that are closed, so no session is told of them again.

UPDATE workspace.code_findings
   SET outcome = 'superseded',
       outcome_at = now()
 WHERE rule_source = 'builtin'
   AND outcome IN ('open', 'ignored')
   AND rule_id IN (
       'rust-file-too-long', 'rust-ignore-needs-reason', 'rust-lint-suppression',
       'rust-log-field-names', 'rust-no-comments', 'rust-query-in-loop',
       'rust-test-name-shape', 'rust-test-unwrap', 'rust-too-many-arguments',
       'rust-unsafe', 'sh-bak-copy', 'sh-bash4-features', 'sh-cargo-clean',
       'sh-flock', 'sh-gnu-date-d', 'sh-gnu-readlink-stat-find', 'sh-grep-perl',
       'sh-hardcoded-home-path', 'sh-no-verify', 'sh-pkill-pgrep-f',
       'sh-push-piped-to-tail', 'sh-sed-inplace', 'sh-shebang-env',
       'ts-no-import-actual-api'
   );
