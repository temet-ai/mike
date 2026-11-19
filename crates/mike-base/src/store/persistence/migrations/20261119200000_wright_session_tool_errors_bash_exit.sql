-- session_tool_errors reported "Bash: (empty error)" every run: a Bash result whose only error is
-- "Exit code N" (an ssh to a sleeping host exits 255, a grep with no match exits 1) is the session's
-- ordinary work, not a tool fault. Such results are skipped. Bash results that carry any other
-- text (permission or hook refusals, timeouts, harness errors) and every other tool still count.
UPDATE wright.checks
   SET query = replace(query,
                       $old$       AND t.body !~ '^[A-Za-z]+(:[^ ]*)? hook error'
)$old$,
                       $new$       AND t.body !~ '^[A-Za-z]+(:[^ ]*)? hook error'
       AND NOT (u.tool_name = 'Bash' AND t.body ~ '^Exit code [0-9]+\s*$')
)$new$),
       expects = expects || '; a Bash result whose only error is a non-zero exit code is not counted',
       updated_at = now()
 WHERE name = 'session_tool_errors'
   AND query NOT LIKE '%u.tool_name = ''Bash'' AND t.body ~%';
