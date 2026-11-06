SELECT pk.code AS kind_code, COUNT(pkc.request_id)::bigint AS turn_count
   FROM prompt_kinds.kinds pk
   LEFT JOIN prompt_kinds.classifications pkc
          ON pkc.kind_id = pk.id AND pkc.session_id = 100749
  GROUP BY pk.code
  ORDER BY pk.code
