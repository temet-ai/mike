SELECT c.id
  FROM wright.causes c
 WHERE c.id = ANY($1::bigint[])
   AND c.merged_into IS NULL
