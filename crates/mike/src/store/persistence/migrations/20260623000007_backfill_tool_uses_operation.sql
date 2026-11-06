-- Backfill tools.uses.operation for historical rows using the canonical
-- tool_name→operation map.  This map is IDENTICAL to the Rust capture-time map
-- in insert_tool_use_row / the UNNEST snapshot path (they must stay in sync).
-- Idempotent: only rows with operation IS NULL are touched.  Set-based.
-- Tools not in the IN-list → operation stays NULL (open allow-list: a later
-- addition + backfill lights them up without touching this migration).
UPDATE tools.uses
SET operation = CASE tool_name
    WHEN 'Read'         THEN 'read'
    WHEN 'Write'        THEN 'write'
    WHEN 'Edit'         THEN 'write'
    WHEN 'MultiEdit'    THEN 'write'
    WHEN 'NotebookEdit' THEN 'write'
    WHEN 'Bash'         THEN 'exec'
    WHEN 'Grep'         THEN 'search'
    WHEN 'Glob'         THEN 'search'
    WHEN 'WebSearch'    THEN 'search'
    WHEN 'WebFetch'     THEN 'fetch'
    WHEN 'Agent'        THEN 'dispatch'
    ELSE NULL END
WHERE operation IS NULL
  AND tool_name IN (
      'Read', 'Write', 'Edit', 'MultiEdit', 'NotebookEdit',
      'Bash', 'Grep', 'Glob', 'WebSearch', 'WebFetch', 'Agent'
  );
