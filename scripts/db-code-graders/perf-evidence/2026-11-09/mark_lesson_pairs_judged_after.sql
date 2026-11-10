UPDATE memory.lesson_pair_candidates p
   SET judged_at = now()
  FROM UNNEST(ARRAY[2,4,6,8]::bigint[], ARRAY[12,14,16,18]::bigint[]) AS u(lesson_a_id, lesson_b_id)
 WHERE p.lesson_a_id = u.lesson_a_id AND p.lesson_b_id = u.lesson_b_id;
