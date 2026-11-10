INSERT INTO memory.lesson_pair_candidates (lesson_a_id, lesson_b_id, score, distance)
SELECT u.lesson_a_id, u.lesson_b_id, u.score, u.distance
  FROM UNNEST(ARRAY[1,2,3,4,5,6]::bigint[], ARRAY[11,12,13,14,15,16]::bigint[],
              ARRAY[0.9,0.8,0.7,0.6,0.55,0.5]::real[], ARRAY[0.3,0.3,0.3,0.3,0.3,0.3]::float8[])
       AS u(lesson_a_id, lesson_b_id, score, distance)
ON CONFLICT (lesson_a_id, lesson_b_id) DO UPDATE
   SET score = EXCLUDED.score, distance = EXCLUDED.distance, found_at = now();
