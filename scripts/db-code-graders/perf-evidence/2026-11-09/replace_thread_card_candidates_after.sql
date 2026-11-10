INSERT INTO work.thread_card_candidates (thread_id, rank, ref, kind, score)
SELECT 7::bigint, u.ord::int, u.ref, u.kind, u.score
  FROM UNNEST(ARRAY['lesson:a','lesson:b','lesson:c','note:d','turn:5','entry:6','lesson:g','lesson:h','lesson:i','lesson:j']::text[],
              ARRAY['lesson','lesson','lesson','note','turn','entry','lesson','lesson','lesson','lesson']::text[],
              ARRAY[0.9,0.8,0.7,0.6,0.5,0.4,0.3,0.2,0.1,0.05]::real[])
       WITH ORDINALITY AS u(ref, kind, score, ord);
