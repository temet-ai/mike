-- A lesson in a standing file is judged like one mike sent, and a verdict says whether the situation
-- came up (not_applicable when it did not) and whether the trouble returned (came_back). Stored
-- verdicts keep their labels.

SET LOCAL lock_timeout = '10s';

ALTER TABLE memory.supplied_items
    DROP CONSTRAINT IF EXISTS supplied_items_route_check;
ALTER TABLE memory.supplied_items
    ADD CONSTRAINT supplied_items_route_check
        CHECK (route IN ('proxy', 'mod', 'tether', 'standing'));

ALTER TABLE memory.supplied_item_verdicts
    DROP CONSTRAINT IF EXISTS supplied_item_verdicts_verdict_check;
ALTER TABLE memory.supplied_item_verdicts
    ADD CONSTRAINT supplied_item_verdicts_verdict_check
        CHECK (verdict IN ('used', 'came_back', 'corrected_anyway', 'ignored', 'not_applicable'));

ALTER TABLE memory.supplied_item_verdicts
    DROP CONSTRAINT IF EXISTS supplied_item_verdicts_hand_check;
ALTER TABLE memory.supplied_item_verdicts
    ADD CONSTRAINT supplied_item_verdicts_hand_check
        CHECK (hand_verdict IS NULL OR hand_verdict IN
               ('used', 'came_back', 'corrected_anyway', 'ignored', 'not_applicable'));

-- A stored copy of the old built-in prompt names the old labels; an edited prompt is left alone.
DELETE FROM config.settings
 WHERE key = 'supply_verdict.system_prompt'
   AND md5(value #>> '{}') = '2704ede49768e6c7a0e1967981032576';

COMMENT ON COLUMN memory.supplied_items.route IS
    'proxy: mike added it to the forwarded body. mod: the mike mod added it as hook context. tether: tether added it. standing: it was in a standing rules file and is close to what the turn was about.';
COMMENT ON COLUMN memory.supplied_item_verdicts.verdict IS
    'used: the turn followed it. came_back: the turn followed it and the operator corrected the turn on what it covers. corrected_anyway: it applied, was not followed, and the operator corrected the turn on it. ignored: it applied and was not followed. not_applicable: what it covers did not come up.';
