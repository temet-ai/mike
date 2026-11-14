UPDATE workspace.code_findings f
   SET outcome = 'superseded',
       outcome_at = now()
 WHERE f.checkout = '/mnt/data/projects/temet/temet-proxy/.claude/worktrees/mike-code-proof'
   AND f.path = ANY(ARRAY['scripts/probe-report.sh','crates/mike-code/src/lib.rs']::text[])
   AND f.outcome IN ('open', 'ignored')
