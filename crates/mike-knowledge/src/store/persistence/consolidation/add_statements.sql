WITH base AS (
    SELECT COALESCE(max(s.ordinal) + 1, 0) AS next
      FROM memory.knowledge_statements s WHERE s.item_id = $1
)
INSERT INTO memory.knowledge_statements
    (item_id, ordinal, text, passage_start, passage_end, embedding, model_key, stated_at, split_hash)
SELECT $1, base.next + (e.n - 1)::integer, e.v ->> 'text',
       (e.v ->> 'passage_start')::integer, (e.v ->> 'passage_end')::integer,
       (e.v ->> 'embedding')::public.vector, $3::text, $4::timestamptz, $2::text
  FROM base, jsonb_array_elements($5::jsonb) WITH ORDINALITY e(v, n)
RETURNING id AS "id!", ordinal AS "ordinal!"
