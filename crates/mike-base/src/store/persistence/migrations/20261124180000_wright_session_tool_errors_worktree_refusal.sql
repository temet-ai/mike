-- session_tool_errors counted Claude Code's worktree-isolation refusals ("This agent is isolated
-- in the worktree …, but this command is too complex to verify that it stays inside the
-- worktree. Refusing to run it") as tool faults (Wright finding 33, problem 73457). Claude Code
-- refuses the command before it runs and the agent recovers by splitting it; nothing in mike
-- emits or can fix it. Such Bash results are skipped.
UPDATE wright.checks
   SET query = replace(query,
                       $old$       AND NOT (u.tool_name = 'Bash' AND t.body ~ '^Exit code [0-9]+\s*$')$old$,
                       $new$       AND NOT (u.tool_name = 'Bash' AND t.body ~ '^Exit code [0-9]+\s*$')
       AND NOT (u.tool_name = 'Bash' AND t.body ~ '^This agent is isolated in the worktree .* Refusing to run it')$new$),
       expects = expects || '; a Bash command Claude Code refuses because it cannot verify it stays inside an agent''s worktree is not counted',
       updated_at = now()
 WHERE name = 'session_tool_errors'
   AND query NOT LIKE '%This agent is isolated in the worktree%';
