-- mcp_tool_errors counted the fleet mailbox's answers to a target that does not exist yet
-- ("task TN has no seat yet", "no other seat of the fleet is open", "the fleet has no task …",
-- from crates/mike-fleet/src/store/persistence/mailbox/fleet_target.rs) as tool faults
-- (temet-ai/mike#455, Wright finding 19, problem 8921). They are documented refusals a seat
-- recovers from by retrying with a session id, and they reopen the problem with every fleet.
-- Such results are skipped.
SET LOCAL lock_timeout = '10s';

UPDATE wright.checks
   SET query = replace(query,
                       $old$       AND u.tool_name LIKE 'mcp\_\_mike\_\_%'$old$,
                       $new$       AND u.tool_name LIKE 'mcp\_\_mike\_\_%'
       AND coalesce(t.body, '') !~ '(has no seat yet: nothing has been started for it|no other seat of the fleet is open|the fleet has no task `)'$new$),
       expects = expects || '; a fleet mailbox refusal to a seat or task that does not exist yet is not counted',
       updated_at = now()
 WHERE name = 'mcp_tool_errors'
   AND query NOT LIKE '%has no seat yet%';
