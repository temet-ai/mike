-- The daily knowledge reconcile compared every source of a kind with its items in two statements,
-- each reading the kind's whole body view, and the second also read all of
-- memory.knowledge_embedding_state once per kind: 274k shared blocks for the eleven built-in kinds
-- (slow_queries problem 37116). One statement per kind now reads the view once, and the state
-- rows of that kind only. A full diff visits every source row, so the function plans with
-- random_page_cost 4: at the server's 1.1 the planner walks each source's primary key and touches
-- a heap page per row instead of reading each table once.
CREATE OR REPLACE FUNCTION memory.reconcile_knowledge_items(p_kind text) RETURNS bigint
LANGUAGE plpgsql
SET random_page_cost = 4
AS $$
DECLARE
    v_view regclass;
    v_changed bigint;
BEGIN
    SELECT to_regclass(k.body_view) INTO v_view FROM memory.knowledge_kinds k WHERE k.kind = p_kind AND k.enabled;
    IF v_view IS NULL THEN
        RETURN 0;
    END IF;
    EXECUTE format(
        'WITH cur AS MATERIALIZED (
             SELECT DISTINCT ON (b.source_id) b.source_id, b.project_id FROM %s b
         ), added AS (
             INSERT INTO memory.knowledge_items (kind, source_id, project_id)
             SELECT $1, c.source_id, c.project_id FROM cur c
              WHERE NOT EXISTS (SELECT 1 FROM memory.knowledge_items i WHERE i.kind = $1 AND i.source_id = c.source_id)
             ON CONFLICT (kind, source_id) DO NOTHING
             RETURNING 1
         ), marked AS (
             UPDATE memory.knowledge_items i SET needs_embedding = true, ready_at = now(), changed_at = clock_timestamp()
               FROM memory.knowledge_embedding_state e
               LEFT JOIN cur ON cur.source_id = e.source_id
              WHERE e.source_kind = $1 AND i.kind = $1 AND e.item_id = i.id AND NOT i.needs_embedding
                AND (cur.source_id IS NULL OR cur.project_id IS DISTINCT FROM e.project_id)
             RETURNING 1
         )
         SELECT (SELECT count(*) FROM added) + (SELECT count(*) FROM marked)', v_view)
    INTO v_changed
    USING p_kind;
    RETURN v_changed;
END
$$;
