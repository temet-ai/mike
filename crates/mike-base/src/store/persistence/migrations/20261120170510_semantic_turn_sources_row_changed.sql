-- `proxy.row_changed` (feed table `semantic_turn_sources`) when a closed turn
-- becomes a source for the semantic worker's turn items: its
-- `memory.turn_facts` row is inserted as a genuine operator prompt, or an
-- existing row is reclassified into one.
--
-- The semantic worker asked for turns without turn items on every tick (every
-- 300 s on each machine). On 2026-11-23 that statement (the
-- `turns_needing_semantic_items` read) ran about 60 times an hour across the
-- five machines and returned nothing. Its rows come only from closed turns
-- whose facts say `prompt_kind = 'genuine'` and `session_class = 'operator'`,
-- and facts are written only for closed turns, so this notice is the only
-- write that can add one; a new embedder key or builder version the worker
-- sees itself. The payload is the same for every row, so a dossier pass that
-- writes facts for many turns in one transaction sends one notice.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_semantic_turn_sources()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'semantic_turn_sources',
        'id',         'memory.turn_facts',
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_turn_facts_semantic_turn_sources_inserted
AFTER INSERT ON memory.turn_facts
FOR EACH ROW
WHEN (NEW.prompt_kind = 'genuine' AND NEW.session_class = 'operator')
EXECUTE FUNCTION mike.notify_row_changed_semantic_turn_sources();

CREATE OR REPLACE TRIGGER trg_turn_facts_semantic_turn_sources_reclassified
AFTER UPDATE OF prompt_kind, session_class ON memory.turn_facts
FOR EACH ROW
WHEN (NEW.prompt_kind = 'genuine' AND NEW.session_class = 'operator'
      AND (OLD.prompt_kind IS DISTINCT FROM NEW.prompt_kind
           OR OLD.session_class IS DISTINCT FROM NEW.session_class))
EXECUTE FUNCTION mike.notify_row_changed_semantic_turn_sources();
