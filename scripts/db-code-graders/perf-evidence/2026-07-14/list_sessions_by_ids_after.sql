WITH ranked AS (
             SELECT s.id,
                    s.session_uuid::text AS session_id,
                    p.project_path,
                    s.first_seen,
                    s.last_seen,
                    s.last_status,
                    s.title,
                    s.pinned,
                    p.pinned AS project_pinned,
                    s.overview,
                    s.overview_generated_at
             FROM sessions.sessions s
             JOIN projects.projects p ON p.id = s.project_id
             WHERE s.session_uuid = ANY('{4f7f36e9-0404-4ac5-b0eb-8e3170e5d530,2b50dddf-d46b-4a76-98cc-e6d3e1941d6a,df05f016-6cc9-4a32-8639-9c0dfb75d0a5,c97a1a5c-05bf-495f-9767-8aee67aecab6,0c6452f6-9ef9-4715-8557-224a3c499fa9,3b9c4acf-e814-4f27-9070-31361a699555,c1f7ca82-4e67-479b-91b4-f0c4ecdd9968,0d83a732-8be2-4e5f-aabf-9ba5d088a1a8,f07269e1-3d9c-4140-aa6e-1e85062d016c,872d7da7-a414-4432-bef2-630c65ef66b5,bbd5feed-8365-465b-af92-f477cf2aa49b,36b263d5-3520-4e11-99a5-707594319b46,54311714-ec51-4df4-8f13-26ee6c7c0b8b,672df85b-5dec-4aea-9148-13ef08017fda,0abf54bb-7ac6-4758-9e3c-c4ff758d2217,de308d66-b7ab-4de3-a3be-35e1ac59a0c9,697018b2-5044-4b17-a7cb-d80dafbfeeb3,0e66116e-1511-4d7d-aa1c-cd1ab3058155,cfc0d8f3-9d96-4b1c-8af5-5644463e1efc,5c3f7c14-ecd4-41ba-b52f-cde5345d34cf,9770e3f6-f3d4-46a1-920f-cc66e4b8860d,328b621c-9db4-420c-99b4-e8ce93207d07,7b01e17f-3f65-442a-8f4d-2475060ab680,d6bfa10c-d2f3-4b13-a2cf-e970d99d2cb2,045ba74e-68e1-47e6-8ed8-56e22a4cd843,5841f1da-3f30-43bc-ac10-977242f39a1b,5282cc68-d231-46b7-8f7f-6e304152fbc4,5668ba2c-70bf-43f8-bff3-4dab2accaa71,5d5b8d89-2b54-4f1e-9827-f24d8895a8d9,dfdcbaf0-bfc6-4444-9b2f-e10eaaf2718c,a404aab1-701f-47bc-a5ca-00b09ea4d83d,50794f0b-911b-4076-8952-1a2adcd8dd03,3cdcb4f7-5f5b-4db8-b7c1-761a5f1cf714,91d00424-7994-4412-b8de-4f55648bb610,a8cc1637-5dc7-4b42-b097-14d934413f03,218d3c12-f49a-4320-bb09-fad13a8157a5,2310696a-5c19-4674-8014-4412d82ff968,65467cdf-97f0-4ff1-9c41-df20d6f4467f,7b8ae47a-39d4-41b3-b2bc-375b59bfaf26,a20c1817-c401-446f-b53f-9ed61a8b2971,f6628c74-c6d9-413a-b4b4-af94a915c733,133a3e3e-9cd9-49fb-9023-061a86aa06dc,999cdb1a-be1a-4a9b-b2e9-076c190759ef,1ac2ef80-201c-4b87-aef2-83f1020933ab,c96eddee-701a-4972-9f89-1504025b95f8,7525f965-df44-48d4-84f5-5a774f96bd64,81980c13-ab3e-4d20-9853-d2e58be3f24e,a7dad9d3-add3-4ce8-9ab1-e12564f7c858,4bd2b16d-2f0b-4fa1-a103-24bfbb1466c3,8821a54d-8a6e-471a-94db-c9ae138c29f0,f67a4588-7660-4a1a-af91-d28057100510,ec825c79-8b29-4a45-9a67-c614e26d286e,ed542d7a-c000-4e76-819e-342188b7f342,e06b252d-ae37-4ab8-b0e2-e466bf9c7e3e,25a58280-b0ab-492b-9928-b7bde6b5e37c,2d7eee15-b700-4ce0-a16f-739ddc98f6d5,83dbce81-1a99-47a0-8ae8-a90def44e6e2,9e312b9d-abc6-42a6-b49b-4d1b97c38051,10f80dbc-29e8-418f-be31-dc4c33edb703,65999e7c-deee-4d22-8319-af4d736b09f3,c9950c40-641e-40e1-a192-b06b9b366b69,d002efc0-4965-458f-b3ea-6ab2d85745d9,a5b38121-063d-4ed9-9c7b-b2ac00c89dd0,045e7904-c168-4338-b968-e9087162dd63,e2b00293-87c5-4e1f-8b4e-569f7ed7f10b,e01b08c2-e382-4114-945e-78c4938df6ae,a719832e-d155-42b0-80dc-e5ac202cc7e5,69e179d1-6cc6-4a25-af01-f45d2d2b6aa9,caf9b7b2-ef88-454d-bc6c-e743f25a87f7,c52c5bee-a47d-49f1-aa2b-8c2545b6f4a2,747b21cd-d6c2-4078-9669-0b4f98ac0f07,c8eb38fb-8eda-42a3-9c22-3fbe27955cc2,4307201d-e694-4889-96fa-b398262e67db,82ce5504-a124-4e3e-9edb-57a625b2cde5,1e1ecc76-d04f-4ff9-b828-5e49be16df28,ae0e40a2-0203-4bca-803a-7bd3cc3e0468,4793b913-8ecc-4c34-859c-d49daf85ce10,7c243d6f-c967-4091-9809-5d2278ac9e8e,de2c4aad-2932-4665-aa43-e665f50616c3,96c6b8d8-e864-462b-893b-765378b4df4e,9911496a-4a8e-43cb-b504-2454f80cfb40,fef81ad3-244f-40e5-9934-4bf897aa0d2a,e73b7851-b279-4ace-ba59-c1c83e512cba,1299b00e-da77-46d6-9279-41df2fbe3a5e,a6392dad-a6c8-4c00-b5fb-6eeb6fddc141,e77573d5-37b3-4330-b588-d676763016c4,139433d0-cea8-4442-b9c9-07597571d43a,99e2ebb1-8198-4dc2-88c2-04942049f06e,0f53f27e-7572-4302-b0d4-9d9a1d2c4125,26a00d48-c45e-4195-8bd6-a14fc5766529,7bf31b69-db4f-475b-ae0e-475e4071dd91,37b7ebd7-69bf-487f-9691-5e77a3dd6212,d4cda4f7-8242-4eee-95ed-2e55e1833b14,e5e0e2bc-0607-4ff9-8b2d-e8f662ccb8b0,b0d0eb12-19cf-49b2-b44e-66ef5ecd4dd2,86c1e679-76da-4aa2-8a51-1d6440cea8b1,ef1bc7a6-a250-4c76-910f-06df54e2cd34,a04f2e9d-e1e3-47c1-b6fb-3dc6604b5718,62a0a1e9-e65e-47f6-93e1-39e513c8ba58,09bf3d8a-d50e-4b0c-917e-3847faf49d58,1849e9fb-b395-43d6-8869-47b83aafabb8,b1f848d2-8fd0-40fb-bacc-28636dc01d2b,4f29b07f-886d-4e8e-884c-406cf6f302e5,0b4a3247-e283-4b4e-b633-3da4e6ca50ab,d64fb559-8555-43af-97d7-64abdf7f406c,2875c812-fc8a-47d6-b3da-6044f5e836d7,dc6047fb-9942-44a3-9e98-942e077285b6,a9aea897-fb0d-40ad-b73a-4a6f20dd823b,1c1e8cc0-95dc-4a53-96fc-a6d3be38fc66,274f05c6-b751-41f4-9ebe-b9e8402433cd,901cf881-61da-47ef-a4ec-7da9b3c6765b,26dceeff-38fb-4857-952c-d53e5f49a5db,ad10f27b-f1a8-4ad9-9382-dd4fd5100104,9ab097c2-0529-46c8-a31f-8ec389d90e9f,ae591a2f-e42e-44db-af1e-83e688f92d1a,87abd05e-b09e-412a-9c35-1a5dd9e0e7db,e9179b23-827c-4819-a066-73d03802604a,b0623f6f-ad18-4eae-8c3a-295f8d82e1ae,7bb1edcf-b3f5-4bad-a60d-f45f576ed33c,56b42008-409b-4146-b23f-f68b0e2a1d89,ff9c45c6-c7a6-4491-90ee-4e40fe725a65,99a83fef-5404-4d96-bcb9-5e31fe111dcf,acf0d33f-df67-4480-858f-30cde970b79a,d2825ec7-13a1-436b-bac7-ec8e95da2f92,dfd5ca6e-56d0-44d6-879f-ac589bbbadf0,c80cbdcb-f845-4d8b-92fe-7fdb3bebd992,8cd45488-7b75-4a13-bd13-502d394e9747,0b482089-8a12-4984-933e-acaae3a0a2a3,0bcc1e97-831a-4379-bad3-7b7feb28ace4,0a6510c7-8eab-4e55-afc3-fe727a913c7e,5251f10a-11c6-4d9e-92a0-71baeb4dc172,d19476df-70d1-418d-8f15-a8b0d0dd9ebd,176737ca-8e2f-425c-8e1d-d79d5aac13da,26e44617-64dd-4089-b3e9-e555fee005ef,1268bdfd-9040-4c6b-8675-e84d2bbee875,53b59e64-71ca-411e-b11e-6b198aac5c50,a9cf13bd-ff96-4662-a613-79c66ec52da3,d3d56e2b-66aa-489f-8141-890cef4a6e63,6a42288d-852e-4ce4-8758-75a98f5b404c,38af4344-4224-4f9c-87cd-db0c14503876,63378ad0-9647-4f3d-a247-129029277f49,da0eb815-63e2-47ae-8dbf-8a21f6b2403d,b48e3f7a-d1e8-414b-89ae-fcc59568180f,c0dc3ad7-84fb-4f2e-9a39-4654610a837f,28dfc377-4ff2-45a8-b726-051b7dab06fb,948b60e1-b50c-4159-8542-1df9c39ef651,dced182b-5191-42f9-a3d6-54168dcf8470,131daf4c-c699-4d88-b1c4-7d9d1bc40f3f,5931960f-b475-4a4b-9623-adf0cf3cd833,88b18838-6165-4da7-8b1a-1c5f66f7e28f,779dc86b-4d1b-44ac-8fd0-972a1290f391,48cabcbf-e0a6-4e95-81b8-bfaf219d1a67,d0325830-2103-41aa-9867-ede4e6e3ad06,93b78e61-2fc4-4969-90d6-58f7919e018d,8e4c0f4e-bca4-4aac-9878-64a47a275217,e79889d5-d05e-4be6-a4ca-ac7b6d006fe7,20338f58-2a29-46fe-bd2f-ce50c71ce795,2e83a052-a100-4051-8985-61e78b8bff55,6e2e33d4-5ad9-4f6a-8140-f69e62c154f6,268e9bd5-dd11-41f5-a799-22f13c278de1,e7ec9ca9-8efd-4103-be67-9661d17354e2,7ec40826-01be-4fa2-9053-3388942acc7b,c1593146-44a9-4ba7-910c-ac16b8005ba9,46837512-860c-49fa-8f32-4b9a14392125,9c385b69-0d9a-4d53-af7d-49b02bd4a21b,70a66350-14df-451f-adba-03df492406a2,4ca28ca7-6cce-4510-b5ca-0ba72333e62b,4d117f7d-bb55-408f-9545-cc530da11e44,b23cd332-2d69-4ea2-8c0e-37694b41cdd4,34105144-7998-4f0e-a4ac-cc03724ad4c1,9e0f9a58-b352-49b5-8c69-f1d96ff001f0,6ee5a48f-7ee0-448e-bbed-1a1fe42316a4,ccf6d79c-c127-4b05-bb81-2d1a0f760026,baef6119-224e-4fb0-bd48-c05b4cfc6efa,f9fbe7c3-a82b-4459-b840-bbc9805e585c,44e5d7ab-fd4f-43e5-84a1-7a7fffa2d71d,bf564e79-a40b-4a7a-ac64-d3e0b802cad8,65f25245-9d8f-454c-8d62-cac5718771b5,d4d3450c-ebef-4ca9-9ee5-99184165a8cf,8e01b463-3e10-4b4d-bf17-d46aa18f98b9,3b893253-cdfa-4755-9f13-7495c88b0cc3,a2d662bd-6f30-4ff4-b648-313875843935,d5808e88-b331-4683-beef-94088270c416,0bfbc7e4-27a8-4d50-b54d-8ce6003af2b9,b7f9e7e9-0155-44f0-9cfc-08206c7204f2,c45e3f33-4510-486f-96fa-549d8ec5c2a4,0831c8ae-f7c8-408b-a2db-b9133c1aa112,e3c1a204-50ef-49d8-8f96-204f7bd70eec,6e302fa1-8a8f-432f-8ad8-0ed96edf1885,d309b653-5065-466a-bf4b-9f82bff2fd49,fef0dedc-daf2-4af1-b65c-443defe3fac8,aefe7efb-ecca-4a48-96b0-fcdac1dffa19,cd7ebea6-9a92-41ff-b1c9-a7f65f92aebe,6f5ad76e-e464-422d-967b-941e4e6fe9ce,45dc0855-0da2-42a0-9fe6-7788feface74,c6740ec0-e606-490b-bed8-2ec782acab67,294c5535-448b-40bf-84e8-c3f9af502484,15f2d1de-e002-407c-b599-96e48772b540,3779d276-2943-47ba-a78a-dc29ca6ea675,02423657-000e-4b0b-8b61-6b376ac89109}'::uuid[])
         )
         SELECT ranked.session_id,
                ranked.project_path,
                ranked.first_seen,
                ranked.last_seen,
                rc.request_count,
                ranked.last_status,
                ARRAY[]::text[] AS last_models,
                ranked.title,
                ranked.pinned,
                ranked.project_pinned,
                NULL::float8 AS avg_latency_ms,
                first_op.user_prompt AS originating_operator_prompt,
                sjq.session_outcome,
                sjq.session_quality_score,
                sjq.session_judged_at,
                ranked.overview,
                ranked.overview_generated_at,
                COALESCE(mbx.pending_inbox_count, 0) AS pending_inbox_count
         FROM ranked
         LEFT JOIN LATERAL (
             -- Per-session request count; bounded to idx_requests_session_id.
             SELECT COUNT(r.id)::int4 AS request_count
             FROM requests.requests r
             WHERE r.session_id = ranked.id
         ) rc ON TRUE
         LEFT JOIN LATERAL (
             -- Drive from classifications (session_id, kind_id='operator') so
             -- idx_pk_classifications_session_kind enables seek+early-stop on
             -- request_id ASC. kind_id resolved via uncorrelated subquery (runs
             -- once). Mirrors the ae88b347 session_head fix.
             SELECT COALESCE(ot.user_prompt, sr.human_prompt) AS user_prompt
               FROM (
                   SELECT pk_cls.request_id
                     FROM prompt_kinds.classifications pk_cls
                    WHERE pk_cls.session_id = ranked.id
                      AND pk_cls.kind_id = (
                          SELECT id FROM prompt_kinds.kinds WHERE code = 'operator'
                      )
                    ORDER BY pk_cls.request_id ASC
                    LIMIT 1
               ) first_cls
               JOIN memory.operator_turns ot
                 ON ot.start_request_id = first_cls.request_id
               LEFT JOIN requests.requests sr ON sr.id = first_cls.request_id
         ) first_op ON TRUE
         LEFT JOIN LATERAL (
             -- Latest session-judge verdict (top-1-per-group, bounded to the
             -- ≤200-id caller cap). Permitted LATERAL per .claude/rules/sql-set-based.md.
             SELECT o.code AS session_outcome,
                    v.quality_score::float8 AS session_quality_score,
                    v.judged_at AS session_judged_at
               FROM session_judges.verdicts v
               JOIN session_judges.outcomes o ON o.id = v.outcome_id
              WHERE v.session_id = ranked.id
              ORDER BY v.judged_at DESC, v.id DESC
              LIMIT 1
         ) sjq ON TRUE
         LEFT JOIN (
             -- Undelivered mailbox count, aggregated once for the whole
             -- ≤200-id page rather than per-row LATERAL — same rationale as
             -- the matching block in list_sessions_paginated.
             SELECT m.to_session_id, COUNT(*)::int4 AS pending_inbox_count
               FROM mailbox.messages m
              WHERE m.sent_request_id IS NULL
              GROUP BY m.to_session_id
         ) mbx ON mbx.to_session_id = ranked.id
         ORDER BY ranked.last_seen DESC
