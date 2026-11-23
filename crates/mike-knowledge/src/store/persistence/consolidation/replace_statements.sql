WITH base AS (
    SELECT COALESCE(max(s.ordinal) + 1, 0) AS next
      FROM memory.knowledge_statements s WHERE s.item_id = $1
), gone_own AS (
    UPDATE memory.knowledge_statements s
       SET state = 'gone', resolved_by_statement_id = NULL
     WHERE s.item_id = $1 AND s.state <> 'gone'
    RETURNING s.id, s.subject_id
), gone_twin AS (
    UPDATE memory.knowledge_statements s
       SET state = 'gone', resolved_by_statement_id = NULL
     WHERE s.state = 'current' AND s.item_id <> $1
       AND s.item_id IN (
           SELECT t.twin_item_id
             FROM memory.knowledge_artifact_twins(ARRAY(
                      SELECT i.source_id FROM memory.knowledge_items i
                       WHERE i.id = $1 AND i.kind = 'artifact')) t)
    RETURNING s.id, s.subject_id
), created AS (
    INSERT INTO memory.knowledge_statements
        (item_id, ordinal, text, passage_start, passage_end, embedding, model_key, stated_at, split_hash)
    SELECT $1, base.next + (e.n - 1)::integer, e.v ->> 'text',
           (e.v ->> 'passage_start')::integer, (e.v ->> 'passage_end')::integer,
           (e.v ->> 'embedding')::public.vector, $3::text, $4::timestamptz, $2::text
      FROM base, jsonb_array_elements($5::jsonb) WITH ORDINALITY e(v, n)
    RETURNING id, ordinal
)
SELECT 'created' AS "outcome!", c.id AS "id!", c.ordinal AS "ordinal?", NULL::bigint AS "subject_id?"
  FROM created c
UNION ALL
SELECT 'gone', g.id, NULL, g.subject_id FROM gone_own g
UNION ALL
SELECT 'gone', g.id, NULL, g.subject_id FROM gone_twin g
