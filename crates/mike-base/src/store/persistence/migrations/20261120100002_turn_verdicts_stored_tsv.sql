-- Stored search vector for live verdicts; see 20261120100000_knowledge_search_stored_tsv.sql. The view gains it.
-- A reader of the view locks the view before the table, so this takes both up front in that order.
LOCK TABLE memory.live_turn_verdicts_v, memory.turn_verdicts IN ACCESS EXCLUSIVE MODE;
ALTER TABLE memory.turn_verdicts
    ADD COLUMN search_tsv tsvector GENERATED ALWAYS AS
        (to_tsvector('english'::regconfig, unmet_ask || ' ' || rationale)) STORED;
CREATE INDEX idx_turn_verdicts_live_search_tsv ON memory.turn_verdicts USING gin (search_tsv) WHERE mode = 'live';
DROP INDEX memory.idx_turn_verdicts_live_text;

CREATE OR REPLACE VIEW memory.live_turn_verdicts_v AS
SELECT v.id, v.operator_turn_id, v.session_id, v.reaction, v.unmet_ask, v.rationale, v.created_at, t.project_id,
       v.search_tsv
  FROM memory.turn_verdicts v
  JOIN memory.operator_turns t ON t.id = v.operator_turn_id
 WHERE v.mode = 'live'
   AND NOT EXISTS (SELECT 1 FROM memory.turn_verdicts n
                    WHERE n.operator_turn_id = v.operator_turn_id AND n.mode = 'live'
                      AND (n.created_at, n.id) > (v.created_at, v.id));
