INSERT INTO requests.author_kinds (id, code) OVERRIDING SYSTEM VALUE
VALUES (101, 'test-perf-check-row')
ON CONFLICT (id) DO NOTHING;
