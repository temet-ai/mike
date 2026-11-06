SELECT steerer_id, COUNT(*)::BIGINT AS active_enrolment_count
FROM steering.enrolments
WHERE disenrolled_at IS NULL
GROUP BY steerer_id
