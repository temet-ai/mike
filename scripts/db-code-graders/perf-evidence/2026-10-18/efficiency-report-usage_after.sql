WITH selected AS (
    SELECT id FROM sessions.sessions
    WHERE session_uuid IN (SELECT value::uuid FROM jsonb_array_elements_text('["05f91b10-d9e3-4f00-8091-ad42508309c0", "0756bb2e-8f9b-4ecb-b381-99539757e198", "105e217c-876f-4f08-af0b-1bf1f57e89b5", "1ffbec1d-52f2-4d48-9a38-fd8636115156", "297876c5-ae67-4fa4-a7fa-e1953896b680", "2b927660-813c-42d0-80aa-50d9c372399f", "2c843c24-8f5f-42c0-897a-2d556f188d0a", "2ff94ef9-4c5b-4b87-a875-c817ab0f25ad", "3145fbeb-9b93-4e2b-b502-87856b34a2ce", "3756159d-2f85-47cb-94f7-fc8d32d973bf", "3803e0b4-2686-4533-869c-dd27cf96035b", "389e9d5e-0b58-4180-86de-40f05877acd2", "3afb703a-fcb5-42e7-9db1-799eca1b4bf0", "3da5874c-798c-46c3-b4b9-92dc9ba0a351", "3da89621-00be-4be8-b4fb-2ce484cce4e5", "403ff713-d0c8-43e4-91fe-640d8584a2af", "4224a512-3f4d-486f-8401-b63a6f993cd6", "46974b9d-00d5-4834-baca-29f2c1058478", "56ab6ad8-bb96-40a4-8348-913f39976dbf", "57eff008-6537-41c6-959d-a1b6971e4045", "5a97f731-6749-4d77-a4cd-86c27a8c0dd2", "63ff98fb-a4e6-4eee-990f-c257c6ebf032", "692563a5-3a3b-448a-90ed-fc4ba9e317b9", "728ee0d8-da4e-4f43-94fa-438ad634e38e", "75e24334-9a93-4fab-a33f-133a95c2bf85", "85222e6e-4981-443e-9dd6-eace9c76b3c3", "87efc021-1dd1-4f8e-8670-f77824e8e4f0", "8c93ca37-6b5c-466d-b86d-0390b405e229", "8d00252e-5119-4561-b8e8-980099179ecc", "90cf1989-ea21-4474-8b9b-a94cec31dfdf", "9b2a1a9d-dc67-449f-8887-431d32ffb22e", "a95b9384-4765-4d5d-a488-3b22304fe018", "b0d40ebe-fd9d-42f0-ae6e-fc4b01eadd00", "b2b9fd1e-ab2c-4b0f-97f1-5c7ed27dedb1", "b36beb44-dc56-44cb-a508-2938a6cd8866", "ba13584f-ade6-4443-8937-6b4a2adb2f60", "ca48f539-8bd8-4567-af0b-4b52149bf53e", "cf363934-6187-405f-8f34-18578206d229", "d527460b-cd80-49eb-a463-481c1b9e577e", "dd56d588-2323-408e-bd1a-7bd89f8068f3", "eecd8255-b915-41ab-b6e8-3038a4ffab72", "f449b978-ee5e-4834-82fe-c073410a2937", "f502c2b0-410e-4170-ba86-0f3588cac255"]'::jsonb))
), cohort AS (
    SELECT r.id FROM requests.requests r JOIN selected s ON s.id = r.session_id
    WHERE r.created_at >= '2026-10-13T12:50:17.411953Z'::timestamptz AND r.created_at <= '2026-10-13T17:01:17.150459Z'::timestamptz
), usage AS (
    SELECT r.id, count(p.id) AS response_rows,
           bool_and(p.input_tokens IS NOT NULL AND p.output_tokens IS NOT NULL
                    AND p.cache_read_input_tokens IS NOT NULL AND p.cache_creation_input_tokens IS NOT NULL
                    AND p.input_tokens >= 0 AND p.output_tokens >= 0
                    AND p.cache_read_input_tokens >= 0 AND p.cache_creation_input_tokens >= 0) AS complete,
           CASE WHEN count(p.id) = 1 THEN
               sum(greatest(p.input_tokens,0) + greatest(p.output_tokens,0)
                   + greatest(p.cache_read_input_tokens,0) + greatest(p.cache_creation_input_tokens,0))
               ELSE 0 END AS observed_tokens
    FROM cohort r LEFT JOIN responses.responses p ON p.request_id = r.id GROUP BY r.id
)
SELECT count(*) AS requests, count(*) FILTER (WHERE response_rows = 1 AND complete) AS complete_requests,
       count(*) FILTER (WHERE response_rows <> 1 OR NOT complete) AS incomplete_requests,
       sum(observed_tokens) AS observed_tokens,
       (SELECT count(*) FROM selected) AS resolved_sessions
FROM usage
