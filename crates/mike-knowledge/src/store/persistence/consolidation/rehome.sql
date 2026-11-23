SELECT outcome AS "outcome!", statement_id AS "statement_id?", range_start AS "range_start?", range_end AS "range_end?"
  FROM memory.rehome_document_statements($1, $2, $3)
