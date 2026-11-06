# fleet-radar-retire-ratatui-db99de21

Exported by `scripts/fleet-run-export fleet-radar-retire-ratatui-db99de21` on 2026-10-21 from the capture database
(`launch.launches`, `workspace.session_notes` of kind decision and report, `launch.tree_checks`,
`launch.fleets`), read-only and time-bounded.

Every free-text field is dropped: no captured text of any kind. Strings
that remain are identifier-shaped tokens (ids, shas, states, kinds, task names); any other string
is null. `launches.json` carries `claude_args_count` in place of the argv. `receipts.json` keeps
the first three words of each command.
