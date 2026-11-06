WITH k AS (
    SELECT id FROM prompt_kinds.kinds WHERE code = 'operator'
), ins AS (
    INSERT INTO prompt_kinds.classifications
           (request_id, session_id, kind_id, rule, repeat_key, classifier_version)
    SELECT 446648, 1173418, k.id, NULL, NULL, 'genuine-v1' FROM k
    ON CONFLICT (request_id) DO NOTHING
    RETURNING 1
)
SELECT count(*) FROM k
