-- The lesson shape check now flags a lesson that carries a calendar date (YYYY-MM-DD): a date
-- records when something happened and goes stale in the standing lessons file. Lessons judged
-- before the check learned this hold a closed "rule" finding and are never looked at again, so
-- drop that finding for the live lessons that carry a date. The shape pass then assesses them
-- afresh and queues each for the rewrite that states the rule without the date.
DELETE FROM memory.lesson_shape_findings f
 USING memory.lessons l
 WHERE l.id = f.lesson_id
   AND f.shape = 'rule'
   AND l.archived_at IS NULL
   AND l.text ~ '(^|[^A-Za-z0-9-])20[0-9]{2}-[0-9]{2}-[0-9]{2}($|[^A-Za-z0-9-])';
