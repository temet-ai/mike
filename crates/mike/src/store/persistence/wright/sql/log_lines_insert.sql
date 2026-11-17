INSERT INTO wright.log_lines (machine_uuid, source, at, level, target, message, raw)
SELECT $1, t.source, t.at, t.level, t.target, t.message, t.raw
  FROM unnest($2::text[], $3::timestamptz[], $4::text[], $5::text[], $6::text[], $7::text[])
       AS t(source, at, level, target, message, raw)
