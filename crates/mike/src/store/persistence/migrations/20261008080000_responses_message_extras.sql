ALTER TABLE responses.responses ADD COLUMN message_extras jsonb;

COMMENT ON COLUMN responses.responses.message_extras IS 'Top-level message fields the stream carried beyond id, model, role, content, stop_reason and usage: input_transformations and stop_details from message_start, stop_sequence from message_delta. Captured verbatim from the wire. NULL when the stream carried none.';
