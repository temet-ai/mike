INSERT INTO memory.supplied_item_verdicts
       (supplied_item_id, verdict, rationale, evidence, model, input_text)
VALUES ($1, $2, $3, $4, $5, $6)
ON CONFLICT (supplied_item_id) DO NOTHING
