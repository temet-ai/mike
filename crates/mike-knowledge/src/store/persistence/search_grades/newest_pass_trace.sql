-- The newest trace any rerank_nightly phase leaves: a search grade, a tuning
-- pass, a thread-card candidate or a lesson pair. Seeds the worker's last run
-- after a restart, so a pass that ran today is not repeated as overdue.
SELECT GREATEST(
    (SELECT max(g.graded_at) FROM memory.search_grades g),
    (SELECT max(p.ran_at) FROM memory.search_tuning_passes p),
    (SELECT max(c.built_at) FROM work.thread_card_candidates c),
    (SELECT max(l.found_at) FROM memory.lesson_pair_candidates l)
) AS newest
