WITH d AS (DELETE FROM work.thread_card_candidates WHERE thread_id = 296 RETURNING *)
INSERT INTO work.thread_card_candidates SELECT * FROM d
