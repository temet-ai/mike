-- Delete blank-string prompt overrides from config.settings (issue #245).
--
-- Three settings hold operator-editable prompt text: `weasel_intercept.
-- correction_prompt`, `judge_worker.system_prompt`, and `session_judge.
-- system_prompt`. Under the rule that applied until now, an empty string
-- saved for one of these was a valid override, and the code that read the
-- setting treated an empty string as "no override" and fell back to the
-- compiled built-in prompt. The operator has decided that a prompt may no
-- longer be blank at all: the settings catalogue default is now the real
-- built-in text instead of an empty placeholder, the server rejects a blank
-- value on write, and the UI refuses a blank save. Under the new rule the
-- only way to mean "use the built-in prompt" is to have no row for the key
-- in this table at all, which is exactly what an absent key already falls
-- back to (see config.settings' table comment, 20260616000002). A leftover
-- blank-string row from before this change would now be a state nothing
-- can produce going forward and that the read path no longer expects, so
-- this migration removes those rows and lets the catalogue default apply
-- explicitly in their place.
--
-- Scoped to the three prompt keys and to rows whose value is a JSON string
-- that is empty once surrounding whitespace is trimmed, so a key holding a
-- non-blank prompt, or any other settings key entirely, is left untouched.
-- Plain DELETE ... WHERE: matches zero rows and does nothing on an empty
-- table or a table with no blank rows, so this is safe to run repeatedly.

DELETE FROM config.settings
WHERE key IN (
    'weasel_intercept.correction_prompt',
    'judge_worker.system_prompt',
    'session_judge.system_prompt'
)
AND jsonb_typeof(value) = 'string'
AND btrim(value #>> '{}') = '';
