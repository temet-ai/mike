-- Issue #355: JC14 J4 shipped advisory delivery for efficiency steerers (append only, no reply
-- demand, a 30-minute time debounce, advice memory, and a check that the recommended tool exists),
-- all keyed on config.class = 'efficiency' and config.requires. The rollout step that was to set
-- those keys by PATCH never ran, so from 2026-09-24 to 2026-09-28 ast-grep-over-grep still pushed
-- "use ast-grep" notes with a reply demand into sessions, three of them into one session in six
-- minutes, while ast-grep was not installed and the session had no LSP tool.
-- This migration makes the classification part of the schema history instead of a manual step.
--
-- Rows are matched by name: ids differ between databases. Each UPDATE only touches a row that does
-- not carry the key yet, so a re-run changes nothing, and an operator's later override survives.
-- Revert: PATCH class back to 'correction' on the steerer.

UPDATE steering.steerers
   SET config = config || '{"class": "efficiency"}'::jsonb,
       updated_at = NOW()
 WHERE name IN ('no-full-spill-read', 'no-cat-in-bash', 'ast-grep-over-grep',
                'monitor-over-sleep', 'tee-then-slice-gates')
   AND deleted_at IS NULL
   AND NOT (config ? 'class');

UPDATE steering.steerers
   SET config = config || '{"requires": [{"binary": "ast-grep"}, {"binary": "sg"}, {"tool_name_contains": "lsp"}]}'::jsonb,
       updated_at = NOW()
 WHERE name = 'ast-grep-over-grep'
   AND deleted_at IS NULL
   AND NOT (config ? 'requires');
