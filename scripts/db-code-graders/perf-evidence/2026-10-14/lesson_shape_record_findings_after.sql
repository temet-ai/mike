INSERT INTO memory.lesson_shape_findings
        (lesson_id, shape, detail, chars, resolution, resolved_at)
 SELECT u.id, u.shape, u.detail, u.chars,
        CASE WHEN u.shape = 'rule' THEN 'none_needed' END,
        CASE WHEN u.shape = 'rule' THEN now() END
   FROM UNNEST(ARRAY[16,18,31,32,33,35,51,52,53,55,56,57,58,59,60,61,107,122,124,144,145,146,147,148,149,150,151,152,153,154,155,198,199,200,201,202,203,204,205,207]::bigint[], ARRAY(SELECT 'rule' FROM generate_series(1,40))::text[], ARRAY(SELECT 'a rule' FROM generate_series(1,40))::text[], ARRAY(SELECT 100 FROM generate_series(1,40))::int[]) AS u(id, shape, detail, chars)
 ON CONFLICT (lesson_id) DO NOTHING
