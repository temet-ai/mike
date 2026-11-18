WITH own_turn AS (
    SELECT max(t.id) AS id
    FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
    WHERE s.session_uuid = NULL::uuid AND (t.opened_by IS NULL OR t.opened_by = 'operator')
), own_refs AS (
    SELECT 'turn:' || o.id AS reference FROM own_turn o WHERE o.id IS NOT NULL
    UNION ALL
    SELECT vk.kind || ':' || v.id
    FROM own_turn o
    JOIN memory.live_turn_verdicts_v v ON v.operator_turn_id = o.id
    CROSS JOIN (VALUES ('unmet_ask'), ('correction')) vk(kind)
), semantic AS (
    SELECT u.reference, u.score FROM unnest(ARRAY['artifact:199562','artifact:199699','artifact:199700','artifact:199701','artifact:199702','artifact:199703','artifact:199738','artifact:199784','artifact:199791','artifact:199793','artifact:199802','artifact:200265','artifact:201516','artifact:201546','artifact:201547','artifact:201548','artifact:201549','artifact:201550','artifact:201593','artifact:201637','artifact:201644','artifact:201646','artifact:202916','artifact:202917','artifact:204066','artifact:204133','artifact:204185','artifact:205213','artifact:205243','artifact:205244','artifact:205245','artifact:205246','artifact:205247','artifact:205290','artifact:205334','artifact:205341','artifact:205343','lesson:0f19dcbf-21cb-45a3-8d38-2bc88619bb57','lesson:42593fd5-4958-40c7-b19b-de59914bc678','lesson:501102bf-2f3c-4f8a-a47a-a410463d9846','lesson:e466269b-2d32-418c-a985-6e081f287464','mail:81b0781b-75d5-4bcc-99c6-4e27ca2c2489','note:5ff06532-ff73-4d91-8567-4cc5f8c4adbe','note:626aac4a-5327-4769-a0ba-6802a357e9d5','note:810284d1-f9f2-47f0-9359-cefdaac4fe3c','note:864a8ae6-e7e6-43fb-8c59-13e7cceb9274','note:9291254a-6506-4130-8ca3-0bf838414c01','note:ba080121-d73d-4dc4-a543-80b046211001','note:c61de089-f524-4468-aae4-01ec99fcd79d','note:cfac1a96-8c2d-4fff-b26a-af0847eb9487','note:fa1eeb49-766b-42a4-8c9d-9a6019988d54','session:02f0199d-683e-40eb-96fb-f116ca541451','session:0efed272-090f-4ae5-8354-c4838186a02c','session:122ace66-be51-4075-9751-588ecda50395','session:16e8abc0-8022-4357-8b4f-855bc67071e6','session:3250a7ba-2413-4d1d-b339-146ed7bcc974','session:41600c2c-ddaa-4b68-b6f1-af3e67eeca78','session:45202882-f180-4ecd-b146-c359f54f26c4','session:47807d12-5b61-433c-a1b4-43c21a5f2527','session:50a2c3d2-c947-47d7-9016-0b8ae1431358','session:5a678ca2-9eed-4f65-9d36-06247a650da4','session:5efa3f7b-c1a7-40fe-bd7f-f11febf90ff0','session:6a950af4-4b72-4646-8558-4cd89b4aa92e','session:6aabc3a0-7229-43c9-a65a-5162ee49d0e9','session:797ef412-3404-4c56-8636-f17ddbdaa075','session:7d77c5c5-f063-452f-9bb6-32db0f214dcc','session:887d6e97-0e15-4697-9632-498886dd5610','session:8e1777bd-1518-431d-bf91-94f58183ac64','session:916d60e7-a6b7-45a6-a1de-ee004b7b76bc','session:97f3ef9e-ad19-4112-aa82-afb51c062631','session:9dda3b31-f7e6-4547-9e13-33b566cfab97','session:b80284bf-7f9d-417d-b3d9-aa6cddbca1a3','session:bb335509-fd78-487c-89b7-a6551284a97b','session:bd8de1e2-f72a-4032-a251-ba0480693e9e','session:c693acee-5ca2-42b1-9610-7fca8ad56792','session:c7fb12c4-360c-4538-a503-1bf45eeab4c4','session:cfcab7b8-5fa2-4ea3-b5e9-35cb2d1fad0e','session:f3b4433b-5b49-496e-a8f0-0d9b2b83a83c','thread:066f29bb-a418-404a-9369-df5d64bca6de','thread:30a0427e-d061-476b-8dc5-95689024c7d8','thread:49b83114-1038-40e9-bfee-03a3a9c7cd77','thread:556651e8-ad91-41fb-8d74-f80fc9ee2177','thread:6716aafe-a4a1-4049-9c57-4a4b424999d5','thread:99230e8d-8bcc-4f59-b5d4-a3032b4cbf73','thread:e8294cf4-156b-40da-9b67-0488a8900ddf','thread:fbd3003a-04a4-4379-9876-dae3c8e87dae','thread:fce74b6e-7a21-4602-8384-f6b8b93babc7','turn:10113','turn:10252','turn:11589','turn:11831','turn:12307','turn:12320','turn:12383','turn:12423','turn:12510','turn:12529','turn:12991','turn:13280','turn:13828','turn:14175','turn:14904','turn:14905','turn:15187','turn:15222','turn:15223','turn:15227','turn:15424','turn:15740','turn:15748','turn:15758','turn:15759','turn:15789','turn:15790','turn:15795','turn:15803','turn:15940','turn:15941','turn:16012','turn:16149','turn:16194','turn:16312','turn:16385','turn:16389','turn:17603','turn:17610','turn:17648','turn:17780','turn:17911','turn:17912','turn:17913','turn:17917','turn:18632','turn:18674','turn:18762','turn:18768','turn:18776','turn:18819','turn:18973','turn:18990','turn:18999','turn:19040','turn:19062','turn:19089','turn:19107','turn:19261','turn:19275','turn:19287','turn:19312','turn:19336','turn:19338','turn:19342','turn:19356','turn:19390','turn:19602','turn:19725','turn:19732','turn:19920','turn:20998','turn:21012','turn:21042','turn:23679','turn:24591','turn:24599','turn:24935','turn:24937','turn:24940','turn:24945','turn:25423','turn:25913','turn:26818','turn:27145','turn:27149','turn:27660','turn:27758','turn:30821','turn:32282','turn:32283','turn:32295','turn:32953','turn:33070','turn:36602','turn:39112','turn:39399','turn:39749','turn:39867','turn:42866','turn:43723','turn:43765','turn:44497','turn:44525','turn:45772','turn:45773','turn:45840','turn:45931','turn:45932','turn:45934','turn:47385','turn:47386','turn:48156','turn:48164','turn:48552','turn:48573','turn:49008','turn:49071','turn:49072','turn:49074','turn:49081','turn:49200','turn:49203','turn:49209','turn:49211','turn:49407','turn:50352','turn:50625','turn:51329','turn:51330','turn:51394','turn:51482','turn:51485','turn:51564','turn:51565','turn:51654','turn:51740','turn:51794','turn:52480','turn:52481','turn:52484','turn:54662','turn:54663','turn:54757','turn:55085','turn:55096','turn:55099','turn:55100','turn:55101','turn:55102','turn:55103','turn:55115','turn:55152','turn:55507','turn:55734','turn:55904','turn:56035','turn:56323','turn:56587','turn:56932','turn:56934','turn:57076','turn:57383','turn:57916','turn:58652','turn:58703','turn:58970']::text[]::text[], ARRAY[0.6270445398377645,0.6355842106915063,0.6270445398377645,0.6270445398377645,0.6270445398377645,0.6045527055883361,0.5812461396671356,0.6269054973339326,0.586512141213938,0.6225050131227863,0.6360409828461437,0.5846027163332534,0.6270445398377645,0.6355842106915063,0.6270445398377645,0.6270445398377645,0.6270445398377645,0.6045527055883361,0.5812461396671356,0.6269054973339326,0.586512141213938,0.5900895527085579,0.6056361017932275,0.6021472036199217,0.6027710813571323,0.6027710813571323,0.6027710813571323,0.6270445398377645,0.6355842106915063,0.6270445398377645,0.6270445398377645,0.6270445398377645,0.6045527055883361,0.5812461396671356,0.6269054973339326,0.586512141213938,0.5900895527085579,0.5963272870682094,0.576843510427436,0.5877494354618749,0.5853067868829314,0.5856381190424975,0.5791342290603878,0.5719932402379138,0.5791102507243534,0.5750263601635801,0.5725484067317117,0.5732426130853483,0.5717286893210777,0.5758156351988639,0.5760149874700904,0.6202248150043864,0.5724113330752316,0.6168752413244593,0.57580384342808,0.5870755380111198,0.6116015364288695,0.6560261817048753,0.5740536798978845,0.5852924410912251,0.6424414237396642,0.5899713566874007,0.6279413374470113,0.583868883789577,0.5740991063685065,0.574809126499039,0.5761867094596396,0.5858248297624926,0.5974872530256138,0.6033688017821145,0.573645702628438,0.5789744618764631,0.5891469051671558,0.6027211489726464,0.5758581179980532,0.5955595069467501,0.5902532759233923,0.5924632593167796,0.6041908220792024,0.5804425669771686,0.5815290039322658,0.5936495596453473,0.6054302524927917,0.6159515383055808,0.5750273801346478,0.5750060684991747,0.5805704381801797,0.5575730107045185,0.49414251622551597,0.501793161088062,0.5244396797811981,0.5110739813755673,0.5946350033255244,0.5249144933939528,0.49126900550375574,0.4926929011598429,0.5131552984052036,0.5241382335795253,0.5413025926629815,0.6743603577221828,0.5362948512140674,0.5526449209797283,0.49257987472291376,0.5129457330726841,0.5698983153367503,0.5783993300672616,0.4949422027119249,0.5036808419994826,0.5031991320128588,0.5057691787871256,0.5179430443491814,0.5362920601281446,0.5192584286573344,0.512396443973808,0.5007817001781543,0.5323607402707996,0.5864078926809053,0.5523914059025122,0.5034771407430472,0.5703880616907433,0.49390970007143875,0.526822755392584,0.5169899832564523,0.5221942222772699,0.5307447678834579,0.5494786455139792,0.5093011108249603,0.49719361868887757,0.5188005743623822,0.543188170885728,0.5360273687929251,0.5247384525134048,0.5713707634398685,0.5018988702682393,0.5631844308839532,0.506740006636384,0.5285363737911041,0.5242932525729853,0.5094023813550783,0.49623703844459355,0.5499021544498545,0.49501030191693385,0.5118903726976038,0.4908696255865622,0.5479815283964264,0.5462324511616927,0.5311524578807311,0.5906227163037423,0.5242885252839344,0.5557880898524902,0.5209057771890154,0.5231974024290502,0.5218012695912367,0.4969403292149974,0.5047253236764261,0.5074270509994337,0.5153606877805889,0.5542336135080403,0.5136394959962194,0.5505976841032229,0.5160052026509204,0.5356678207020156,0.4995349198164243,0.5026630022911159,0.515692481811342,0.55428206562921,0.5546653366082924,0.49450661389363926,0.5078218437274231,0.6074768870234353,0.4921055885522384,0.5226560986833994,0.4955956777682933,0.5280093774820743,0.5336546303278048,0.5398514157401091,0.5486826859798574,0.5559569730656172,0.5245407497046237,0.4912144374400801,0.49601496649190435,0.49350313861663686,0.4993800370961663,0.5525373039583635,0.5283393207708775,0.6991165905401419,0.5473934361861483,0.49952395241890524,0.5079159159459948,0.6100225842079833,0.5449992905556814,0.5829913541369174,0.5829913541369174,0.4966484000231063,0.5177274788103776,0.5177274788103776,0.5371703069559234,0.5209352243783061,0.5209352243783061,0.567270907054381,0.5354005938352806,0.5437517212711521,0.5215878900463028,0.49679285211393487,0.5367973374599742,0.5367973374599742,0.5534087318249701,0.5484672631225813,0.5012946575494713,0.5192801270509203,0.4977610258246653,0.6219041638145937,0.6360054909008886,0.5383509695101563,0.5689765713394221,0.5107660737405908,0.5076198426598865,0.5581205722869496,0.5337708868355324,0.5139127560023962,0.5069304427297279,0.4938641771946455,0.4997947811101833,0.5156069300078243,0.5612182240074493,0.5248118795398832,0.5160681778294298,0.5269536001259862,0.49880380903562993,0.5286747196352721,0.5041035273291188,0.574482203166957,0.5673386188163692,0.5392302663282027,0.5392302663282027,0.5392302663282027,0.5392302663282027,0.5392302663282027,0.5390448823989672,0.5242805353654192,0.4989504215599192,0.5412106401810913,0.6659140185226977,0.5416439560587147,0.5080941934810227,0.5371917734958578,0.535938272672323,0.5027085705413152,0.5332401593387556,0.533188160212929,0.5064895605882882,0.49332439740286604,0.4948114009258229,0.5616703053805695]::float8[]::float8[]) AS u(reference, score)
), idents AS (
    SELECT h.txt, h.pos, h.is_hash,
           '(^|[^[:alnum:]])' || regexp_replace(h.txt, '([^[:alnum:]])', '\\\1', 'g')
           || CASE WHEN h.is_hash THEN '($|[^[:alnum:]]|[0-9a-fA-F])' ELSE '($|[^[:alnum:]])' END AS pattern
    FROM (SELECT u.txt, u.pos,
                 u.txt ~ '^[0-9a-fA-F]{7,40}$' AND u.txt ~ '[0-9]' AND u.txt ~ '[a-fA-F]' AS is_hash
            FROM unnest(ARRAY[]::text[]::text[]) WITH ORDINALITY AS u(txt, pos)) h
), ident_tsq AS (
    SELECT string_agg('(' || i.tsq::text || ')', ' | ')::tsquery AS tsq
    FROM (SELECT CASE WHEN d.is_hash THEN to_tsquery('english', lower(d.txt) || ':*')
                      ELSE phraseto_tsquery('english', d.txt) END AS tsq
            FROM idents d) i
    WHERE numnode(i.tsq) > 0
), q AS (
    SELECT base.tsq, base.pool,
           CASE WHEN ident_tsq.tsq IS NULL THEN base.tsq ELSE base.tsq || ident_tsq.tsq END AS match_tsq,
           ident_tsq.tsq AS ident_tsq
      FROM (SELECT CASE WHEN p.pool THEN replace(w::text, ' & ', ' | ')::tsquery ELSE w END AS tsq, p.pool
              FROM websearch_to_tsquery('english', 'fleet seat session work') w
             CROSS JOIN LATERAL (SELECT true::bool AND strpos(w::text, '!') = 0 AND numnode(w) > 0 AS pool) p) base
     CROSS JOIN ident_tsq
), terms AS (
    SELECT row_number() OVER () AS ti, u.tq, u.ident
      FROM (SELECT DISTINCT btrim(t.txt, '() ')::tsquery AS tq, false AS ident
              FROM q CROSS JOIN LATERAL unnest(string_to_array(q.tsq::text, ' | ')) AS t(txt)
             WHERE q.pool
            UNION ALL
            SELECT q.ident_tsq, true FROM q WHERE q.pool AND q.ident_tsq IS NOT NULL) u
), pool_probe AS (
    SELECT 'artifact' COLLATE "C" AS kind, h.id::text COLLATE "C" AS k1, NULL::text COLLATE "C" AS k2,
           NULL::text COLLATE "C" AS k3, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT r0.id
                           FROM (SELECT r1.id FROM memory.knowledge_revisions r1 WHERE r1.search_tsv @@ t.tq OFFSET 0) r0
                           JOIN memory.knowledge_documents d0 ON d0.current_revision = r0.id
                           JOIN memory.knowledge_sources s0 ON s0.source_uuid = d0.source_uuid
                           LEFT JOIN projects.locations l0 ON l0.location_uuid = s0.location_uuid
                          WHERE l0.project_id = 993478 OR s0.machine_uuid IS NOT NULL) h
    UNION ALL
    SELECT 'lesson', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT l0.id FROM memory.lessons l0
                          WHERE l0.project_id = 993478 AND to_tsvector('english', l0.text) @@ t.tq) h
    UNION ALL
    SELECT 'note', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT n0.id FROM workspace.session_notes n0 JOIN sessions.sessions s0 ON s0.id = n0.session_id
                          WHERE n0.search_tsv @@ t.tq AND s0.project_id = 993478) h
    UNION ALL
    SELECT 'turn', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT t0.id FROM memory.operator_turns t0
                          WHERE t0.project_id = 993478 AND t0.search_tsv @@ t.tq
                            AND (t0.opened_by IS NULL OR t0.opened_by = 'operator')) h
    UNION ALL
    SELECT 'thread_msg', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT m0.id FROM mailbox.messages m0 JOIN mailbox.threads p0 ON p0.id = m0.thread_id
                          WHERE m0.search_tsv @@ t.tq AND p0.project_id = 993478) h
    UNION ALL
    SELECT 'thread_entry', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT e0.id FROM work.thread_log e0 JOIN work.threads w0 ON w0.id = e0.thread_id
                          WHERE e0.search_tsv @@ t.tq AND w0.project_id = 993478) h
    UNION ALL
    SELECT 'question_answer', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT a0.id FROM workspace.questions a0
                          WHERE a0.project_id = 993478
                            AND to_tsvector('english', a0.question || ' ' || COALESCE(a0.answer, '')) @@ t.tq) h
    UNION ALL
    SELECT 'verdict', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT v0.id FROM memory.turn_verdicts v0 JOIN memory.operator_turns vt0 ON vt0.id = v0.operator_turn_id
                          WHERE v0.mode = 'live' AND vt0.project_id = 993478
                            AND to_tsvector('english', v0.unmet_ask || ' ' || v0.rationale) @@ t.tq) h
    UNION ALL
    SELECT 'learning', h.session_id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT k0.session_id FROM memory.session_knowledge k0
                           JOIN sessions.sessions s0 ON s0.session_uuid = k0.session_id
                          WHERE s0.project_id = 993478
                            AND to_tsvector('english', memory.session_learnings_text(k0.learnings)) @@ t.tq) h
    UNION ALL
    SELECT 'session', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT s0.id FROM sessions.sessions s0
                          WHERE s0.project_id = 993478 AND to_tsvector('english', s0.overview) @@ t.tq) h
    UNION ALL
    SELECT 'mail', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT m0.id FROM mailbox.messages m0
                           JOIN sessions.sessions f0 ON f0.id = m0.from_session_id
                           LEFT JOIN mailbox.threads p0 ON p0.id = m0.thread_id
                          WHERE m0.from_session_id IS NOT NULL AND m0.search_tsv @@ t.tq
                            AND COALESCE(p0.project_id, f0.project_id) = 993478) h
    UNION ALL
    SELECT 'seat', h.item_kind, h.source_ref, h.session_uuid::text, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT i0.item_kind, i0.source_ref, i0.session_uuid FROM memory.seat_memory_items i0
                          WHERE i0.project_id = 993478 AND i0.search_tsv @@ t.tq) h
), later_lessons AS (
    SELECT DISTINCT ON (rel.to_lesson_id, rk.code) rel.to_lesson_id, rk.code AS relation, rs.code AS status,
           'lesson:' || newer.uuid AS by_reference
    FROM memory.lesson_relations rel
    JOIN memory.lesson_relation_kinds rk ON rk.id = rel.kind_id AND rk.code IN ('supersedes', 'contradicts')
    JOIN memory.lesson_relation_statuses rs ON rs.id = rel.status_id AND rs.code IN ('confirmed', 'proposed')
    JOIN memory.lessons newer ON newer.id = rel.from_lesson_id
    ORDER BY rel.to_lesson_id, rk.code, rs.code = 'confirmed' DESC, rel.id DESC
), lesson_marks AS (
    SELECT ll.to_lesson_id,
           bool_or(ll.relation = 'supersedes' AND ll.status = 'confirmed') AS superseded,
           jsonb_object_agg(CASE ll.relation WHEN 'supersedes' THEN 'superseded_by' ELSE 'contradicted_by' END,
                            jsonb_build_object('reference', ll.by_reference, 'status', ll.status)) AS marks
    FROM later_lessons ll
    GROUP BY ll.to_lesson_id
), thread_scope AS (
    SELECT wt.id AS thread_id, wt.slug FROM work.threads wt WHERE wt.project_id = 993478 AND wt.slug = NULL::text
), thread_scope_sessions AS (
    SELECT ts.session_uuid::text AS session_uuid
    FROM work.thread_sessions ts JOIN thread_scope t ON t.thread_id = ts.thread_id
    UNION
    SELECT l.session_uuid::text FROM work.thread_log l JOIN thread_scope t ON t.thread_id = l.thread_id
     WHERE l.session_uuid IS NOT NULL
), source_freshness AS (
    SELECT s.source_uuid, jsonb_build_object(
               'state', CASE WHEN jsonb_array_length(fr.reasons) = 0 THEN 'fresh' ELSE 'stale' END,
               'reasons', fr.reasons,
               'last_scan_at', s.last_scan_at,
               'last_verified_at', GREATEST(s.last_scan_at, s.last_heartbeat_at)) AS freshness
      FROM memory.knowledge_sources s
      LEFT JOIN projects.locations sl ON sl.location_uuid = s.location_uuid
     CROSS JOIN LATERAL (
        SELECT COALESCE(jsonb_agg(f.reason), '[]'::jsonb) AS reasons
          FROM (VALUES
            (CASE WHEN s.last_failure IS NOT NULL AND s.last_failure_generation = s.generation
                  THEN 'the source''s latest scan failed: ' || s.last_failure END),
            (CASE WHEN GREATEST(s.last_scan_at, s.last_heartbeat_at) IS NULL
                  THEN 'the source has never been scanned'
                  WHEN GREATEST(s.last_scan_at, s.last_heartbeat_at) < now() - interval '1 hour'
                  THEN 'no scanner has checked the source since '
                       || to_char(GREATEST(s.last_scan_at, s.last_heartbeat_at) AT TIME ZONE 'UTC',
                                  'YYYY-MM-DD"T"HH24:MI:SS"Z"') END)
          ) AS f(reason)
         WHERE f.reason IS NOT NULL
     ) fr
     WHERE sl.project_id = 993478 OR s.machine_uuid IS NOT NULL
), bound AS (
    -- The page needs the best N = limit + offset rows. A row beyond position M in both the words and the
    -- meaning list scores below 2/(60 + M), under half of the worst possible score of the N-th row (half,
    -- because rows ranked lower count at 0.5), so only the best M of each list, and the newest M of the
    -- rows without any score (an empty query), are kept.
    -- A date range or a work thread is applied after the lists are bounded, so the bound is widened while one is
    -- given.
    SELECT CASE WHEN NULL::text IS NULL AND NULL::timestamptz IS NULL AND NULL::timestamptz IS NULL
                THEN 4 * (51::bigint + 0::bigint) + 180 ELSE 2000 END AS m
), pool_weight AS (
    SELECT c.kind, c.ti, t.ident, ln(1 + (max(c.df) OVER (PARTITION BY c.kind))::float8 / c.df) AS w
      FROM (SELECT pp.kind, pp.ti, count(*) AS df FROM pool_probe pp GROUP BY pp.kind, pp.ti) c
      JOIN terms t ON t.ti = c.ti
), pool_pick AS MATERIALIZED (
    SELECT d.kind, d.k1, d.k2, d.k3
      FROM (SELECT s.kind, s.k1, s.k2, s.k3, s.ident,
                   row_number() OVER (PARTITION BY s.kind ORDER BY s.weight DESC, s.k1 DESC, s.k2, s.k3) AS pos
              FROM (SELECT pp.kind, pp.k1, pp.k2, pp.k3, sum(pw.w) AS weight, bool_or(pw.ident) AS ident
                      FROM pool_probe pp JOIN pool_weight pw ON pw.kind = pp.kind AND pw.ti = pp.ti
                     GROUP BY pp.kind, pp.k1, pp.k2, pp.k3) s) d
     WHERE d.pos <= (SELECT m FROM bound) OR d.ident
), kind_hit_ids AS (
    SELECT h.id
      FROM memory.knowledge_embedding_state h
      JOIN memory.knowledge_kinds nk ON nk.kind = h.source_kind AND NOT nk.built_in
     WHERE 'fleet seat session work' <> '' AND h.project_id = 993478 AND h.completed_at IS NOT NULL
       AND h.search_tsv @@ (SELECT match_tsq FROM q)
       AND (NULL::text IS NULL OR h.source_kind = NULL::text)
    UNION
    SELECT h.id
      FROM memory.knowledge_kinds nk
      JOIN semantic s2 ON starts_with(s2.reference, nk.kind || ':')
      JOIN memory.knowledge_embedding_state h ON h.reference = s2.reference AND h.source_kind = nk.kind
     WHERE NOT nk.built_in AND h.project_id = 993478 AND h.completed_at IS NOT NULL
       AND (NULL::text IS NULL OR h.source_kind = NULL::text)
    UNION
    (SELECT newest.state_id
       FROM memory.knowledge_kinds nk
      CROSS JOIN LATERAL (
          SELECT e.id AS state_id, hi.changed_at
            FROM memory.knowledge_items hi
            JOIN memory.knowledge_embedding_state e ON e.item_id = hi.id AND e.completed_at IS NOT NULL
           WHERE 'fleet seat session work' = '' AND hi.kind = nk.kind AND hi.project_id = 993478
             AND (NULL::text IS NULL OR hi.kind = NULL::text)
           ORDER BY hi.changed_at DESC
           LIMIT (SELECT m FROM bound)) newest
      WHERE NOT nk.built_in
      ORDER BY newest.changed_at DESC
      LIMIT (SELECT m FROM bound))
), kind_hits AS MATERIALIZED (
    SELECT e.id, e.source_kind, e.source_id, e.reference, e.project_id, e.search_tsv, i.changed_at
      FROM kind_hit_ids k
      JOIN memory.knowledge_embedding_state e ON e.id = k.id
      JOIN memory.knowledge_items i ON i.id = e.item_id
), kind_bodies AS MATERIALIZED (
    SELECT g.kind, b.source_id, b.label, b.name, b.body
      FROM (SELECT h.source_kind AS kind, array_agg(h.source_id) AS ids FROM kind_hits h GROUP BY h.source_kind) g
     CROSS JOIN LATERAL memory.knowledge_item_bodies(g.kind, g.ids) b
), candidates AS (
    SELECT COALESCE(r.search_tsv @@ q.ident_tsq, false) AS is_ident,
           'artifact:' || r.id AS reference, 'artifact' AS source_kind, r.title,
           r.status, d.relative_path AS path, r.observed_at AS updated_at,
           ts_rank_cd(r.search_tsv, q.tsq) AS rank, COALESCE(sem.score, 0) AS semantic_rank,
           jsonb_build_object('document_uuid', d.document_uuid, 'revision', r.id,
             'location_uuid', s.location_uuid, 'machine_uuid', s.machine_uuid, 'last_scan_at', s.last_scan_at,
             'deleted', d.deleted_at IS NOT NULL, 'metadata', r.metadata,
             'freshness', sf.freshness) AS provenance,
           r.body AS body, NULL::jsonb AS author,
           s.name || ':' || d.relative_path AS dedupe_key,
           NULL::bigint AS turn_id, NULL::text AS turn_prompt, false AS answered
      FROM (SELECT h.id FROM memory.knowledge_revisions h
             WHERE 'fleet seat session work' <> '' AND NOT (SELECT pool FROM q) AND h.search_tsv @@ (SELECT match_tsq FROM q)
            UNION
            SELECT ph.k1::bigint FROM pool_pick ph WHERE ph.kind = 'artifact'
            UNION
            SELECT split_part(s2.reference, ':', 2)::bigint FROM semantic s2
             WHERE s2.reference LIKE 'artifact:%'
            UNION
            (SELECT d0.current_revision FROM memory.knowledge_documents d0
              JOIN memory.knowledge_sources s0 ON s0.source_uuid = d0.source_uuid
              LEFT JOIN projects.locations l0 ON l0.location_uuid = s0.location_uuid
              JOIN memory.knowledge_revisions r0 ON r0.id = d0.current_revision
             WHERE 'fleet seat session work' = '' AND (l0.project_id = 993478 OR s0.machine_uuid IS NOT NULL)
             ORDER BY r0.observed_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN memory.knowledge_revisions r ON r.id = hit.id
      JOIN memory.knowledge_documents d ON d.document_uuid = r.document_uuid AND d.current_revision = r.id
      JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
      LEFT JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
      JOIN source_freshness sf ON sf.source_uuid = s.source_uuid
      CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'artifact:' || r.id
     WHERE (loc.project_id = 993478 OR s.machine_uuid IS NOT NULL)
       AND ('fleet seat session work' = '' OR r.search_tsv @@ q.match_tsq OR sem.score IS NOT NULL)
       AND (NULL::text IS NULL OR NULL::text = 'artifact')
       AND s.name NOT IN ('claude-memory', 'skills')
       AND (false OR (d.deleted_at IS NULL AND r.status NOT IN ('superseded', 'archived', 'rejected')))
    UNION ALL
    SELECT COALESCE(r.search_tsv @@ q.ident_tsq, false),
           'artifact:' || r.id AS reference,
           CASE s.name WHEN 'claude-memory' THEN 'memory' ELSE 'skill' END AS source_kind, r.title,
           r.status, d.relative_path AS path, r.observed_at AS updated_at,
           ts_rank_cd(r.search_tsv, q.tsq) AS rank, COALESCE(sem.score, 0) AS semantic_rank,
           jsonb_build_object('document_uuid', d.document_uuid, 'revision', r.id,
             'location_uuid', s.location_uuid, 'machine_uuid', s.machine_uuid, 'last_scan_at', s.last_scan_at,
             'deleted', d.deleted_at IS NOT NULL, 'metadata', r.metadata,
             'freshness', sf.freshness) AS provenance,
           r.body AS body, NULL::jsonb AS author,
           s.name || ':' || d.relative_path AS dedupe_key,
           NULL::bigint AS turn_id, NULL::text AS turn_prompt, false AS answered
      FROM (SELECT h.id FROM memory.knowledge_revisions h
             WHERE 'fleet seat session work' <> '' AND NOT (SELECT pool FROM q) AND h.search_tsv @@ (SELECT match_tsq FROM q)
            UNION
            SELECT ph.k1::bigint FROM pool_pick ph WHERE ph.kind = 'artifact'
            UNION
            SELECT split_part(s2.reference, ':', 2)::bigint FROM semantic s2
             WHERE s2.reference LIKE 'artifact:%'
            UNION
            (SELECT d0.current_revision FROM memory.knowledge_documents d0
              JOIN memory.knowledge_sources s0 ON s0.source_uuid = d0.source_uuid
              LEFT JOIN projects.locations l0 ON l0.location_uuid = s0.location_uuid
              JOIN memory.knowledge_revisions r0 ON r0.id = d0.current_revision
             WHERE 'fleet seat session work' = '' AND (l0.project_id = 993478 OR s0.machine_uuid IS NOT NULL)
             ORDER BY r0.observed_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN memory.knowledge_revisions r ON r.id = hit.id
      JOIN memory.knowledge_documents d ON d.document_uuid = r.document_uuid AND d.current_revision = r.id
      JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
      LEFT JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
      JOIN source_freshness sf ON sf.source_uuid = s.source_uuid
      CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'artifact:' || r.id
     WHERE (loc.project_id = 993478 OR s.machine_uuid IS NOT NULL)
       AND ('fleet seat session work' = '' OR r.search_tsv @@ q.match_tsq OR sem.score IS NOT NULL)
       AND (NULL::text IS NULL OR NULL::text IN ('memory', 'skill'))
       AND s.name IN ('claude-memory', 'skills')
       AND (false OR (d.deleted_at IS NULL AND r.status NOT IN ('superseded', 'archived', 'rejected')))
    UNION ALL
    SELECT COALESCE(lt.tsv @@ q.ident_tsq, false),
           'lesson:' || l.uuid, 'lesson', btrim(split_part(btrim(l.text), E'\n', 1)), st.code,
           NULL, l.created_at,
           ts_rank_cd(lt.tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('lesson_id', l.id, 'lesson_kind', k.code, 'topic', l.topic,
             'source_turn_id', l.source_turn_id, 'session_uuid', lss.session_uuid)
             || COALESCE(lm.marks, '{}'::jsonb)
             || CASE WHEN l.retired_at IS NOT NULL THEN jsonb_build_object('retired', jsonb_build_object(
                    'kind', l.retired_kind, 'reason', l.retired_reason, 'at', l.retired_at,
                    'by_lesson', 'lesson:' || rby.uuid)) ELSE '{}'::jsonb END,
           l.text, CASE WHEN lss.session_uuid IS NOT NULL THEN jsonb_build_object('role', 'lesson',
             'session_uuid', lss.session_uuid, 'session_title', lss.title) END,
           'lesson:' || l.uuid, NULL, NULL, false
      FROM (SELECT h.id FROM memory.lessons h
             WHERE 'fleet seat session work' <> '' AND NOT (SELECT pool FROM q) AND h.project_id = 993478
               AND to_tsvector('english', h.text) @@ (SELECT match_tsq FROM q)
            UNION
            SELECT ph.k1::bigint FROM pool_pick ph WHERE ph.kind = 'lesson'
            UNION
            SELECT l0.id FROM semantic s2 JOIN memory.lessons l0 ON l0.uuid = substr(s2.reference, 8)::uuid
             WHERE s2.reference LIKE 'lesson:%'
            UNION
            SELECT h.id FROM memory.lessons h WHERE 'fleet seat session work' = '' AND h.project_id = 993478) hit
      JOIN memory.lessons l ON l.id = hit.id
      JOIN memory.lesson_kinds k ON k.id = l.kind_id
      JOIN memory.lesson_stages st ON st.id = l.stage_id
      LEFT JOIN lesson_marks lm ON lm.to_lesson_id = l.id
      LEFT JOIN memory.lessons rby ON rby.id = l.retired_by_lesson_id
      LEFT JOIN memory.operator_turns lst ON lst.id = l.source_turn_id
      LEFT JOIN sessions.sessions lss ON lss.id = COALESCE(l.session_id, lst.session_id)
     CROSS JOIN LATERAL (SELECT to_tsvector('english', l.text) AS tsv OFFSET 0) lt
      CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'lesson:' || l.uuid
     WHERE l.project_id = 993478 AND (NULL::text IS NULL OR NULL::text = 'lesson')
       AND ('fleet seat session work' = '' OR lt.tsv @@ q.match_tsq OR sem.score IS NOT NULL)
       AND (false OR (l.archived_at IS NULL AND (l.expires_at IS NULL OR l.expires_at > now())
         AND NOT COALESCE(lm.superseded, false)))
    UNION ALL
    SELECT COALESCE(n.search_tsv @@ q.ident_tsq, false),
           'note:' || n.note_uuid, 'note', n.title, COALESCE(n.status, 'unknown'), NULL,
           n.updated_at, ts_rank_cd(n.search_tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('session_uuid', ss.session_uuid, 'kind', n.kind, 'tags', n.tags),
           n.body, jsonb_build_object('role', 'session', 'session_uuid', ss.session_uuid,
             'session_title', ss.title),
           'note:' || n.note_uuid, NULL, NULL, false
      FROM (SELECT h.id FROM workspace.session_notes h
             WHERE 'fleet seat session work' <> '' AND NOT (SELECT pool FROM q) AND h.search_tsv @@ (SELECT match_tsq FROM q)
            UNION
            SELECT ph.k1::bigint FROM pool_pick ph WHERE ph.kind = 'note'
            UNION
            SELECT n0.id FROM semantic s JOIN workspace.session_notes n0 ON n0.note_uuid = substr(s.reference, 6)::uuid
             WHERE s.reference LIKE 'note:%'
            UNION
            (SELECT h.id FROM workspace.session_notes h JOIN sessions.sessions hs ON hs.id = h.session_id
             WHERE 'fleet seat session work' = '' AND hs.project_id = 993478
             ORDER BY h.updated_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN workspace.session_notes n ON n.id = hit.id
      JOIN sessions.sessions ss ON ss.id = n.session_id CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'note:' || n.note_uuid
     WHERE ss.project_id = 993478 AND (NULL::text IS NULL OR NULL::text = 'note')
       AND ('fleet seat session work' = '' OR n.search_tsv @@ q.match_tsq OR sem.score IS NOT NULL)
       AND (false OR COALESCE(n.status, '') NOT IN ('superseded', 'archived', 'rejected'))
    UNION ALL
    SELECT COALESCE(t.search_tsv @@ q.ident_tsq, false),
           'turn:' || t.id, 'turn', COALESCE(t.user_prompt, ''), 'historical', NULL,
           t.created_at, ts_rank_cd(t.search_tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('session_uuid', ss.session_uuid),
           t.user_prompt, jsonb_build_object('role', 'operator', 'session_uuid', ss.session_uuid,
             'session_title', ss.title),
           'turn:' || t.id, t.id, t.user_prompt, COALESCE(octet_length(t.final_assistant_text), 0) > 0
      FROM (SELECT h.id FROM memory.operator_turns h
             WHERE 'fleet seat session work' <> '' AND NOT (SELECT pool FROM q) AND h.project_id = 993478 AND h.search_tsv @@ (SELECT match_tsq FROM q)
            UNION
            SELECT ph.k1::bigint FROM pool_pick ph WHERE ph.kind = 'turn'
            UNION
            SELECT split_part(s.reference, ':', 2)::bigint FROM semantic s
             WHERE s.reference LIKE 'turn:%'
            UNION
            (SELECT h.id FROM memory.operator_turns h WHERE 'fleet seat session work' = '' AND h.project_id = 993478
             ORDER BY h.created_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN memory.operator_turns t ON t.id = hit.id
      JOIN sessions.sessions ss ON ss.id = t.session_id CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'turn:' || t.id
     WHERE t.project_id = 993478 AND (NULL::text IS NULL OR NULL::text = 'turn')
       AND ('fleet seat session work' = '' OR t.search_tsv @@ q.match_tsq OR sem.score IS NOT NULL)
       AND (t.opened_by IS NULL OR t.opened_by = 'operator')
    UNION ALL
    SELECT COALESCE(hit.is_ident, false),
           'thread:' || th.thread_uuid, 'thread', th.subject,
           CASE WHEN th.resolved_at IS NULL THEN 'open' ELSE 'resolved' END, NULL,
           th.last_message_at, COALESCE(hit.rank, 0), COALESCE(sem.score, 0),
           jsonb_build_object('thread_uuid', th.thread_uuid, 'fleet_uuid', th.fleet_uuid),
           th.subject, NULL::jsonb, 'thread:' || th.thread_uuid, NULL, NULL, false
      FROM mailbox.threads th CROSS JOIN q
      LEFT JOIN (SELECT m.thread_id, max(ts_rank_cd(m.search_tsv, q.tsq)) AS rank,
                        bool_or(COALESCE(m.search_tsv @@ q.ident_tsq, false)) AS is_ident
                   FROM mailbox.messages m JOIN mailbox.threads pt ON pt.id = m.thread_id CROSS JOIN q
                  WHERE 'fleet seat session work' <> '' AND pt.project_id = 993478 AND m.search_tsv @@ q.match_tsq
                    AND (NOT q.pool OR m.id IN (SELECT ph.k1::bigint FROM pool_pick ph WHERE ph.kind = 'thread_msg'))
                  GROUP BY m.thread_id) hit ON hit.thread_id = th.id
      LEFT JOIN semantic sem ON sem.reference = 'thread:' || th.thread_uuid
     WHERE th.project_id = 993478 AND (NULL::text IS NULL OR NULL::text = 'thread')
       AND ('fleet seat session work' = '' OR hit.thread_id IS NOT NULL OR sem.score IS NOT NULL)
    UNION ALL
    SELECT COALESCE(doc.tsv @@ q.ident_tsq, false),
           'work_thread:' || wt.slug, 'work_thread', COALESCE(wt.title, wt.slug),
           COALESCE(wt.status, 'unknown'), NULL,
           CASE WHEN wlast.entry_date IS NOT NULL THEN wlast.entry_date::timestamp AT TIME ZONE 'UTC'
                ELSE wt.updated_at END,
           ts_rank_cd(doc.tsv, q.tsq), 0::double precision,
           jsonb_build_object('thread_id', wt.id, 'slug', wt.slug, 'goal', wt.goal),
           doc.txt, NULL::jsonb, 'work_thread:' || wt.slug, NULL, NULL, false
      FROM work.threads wt LEFT JOIN work.thread_state wts ON wts.thread_id = wt.id CROSS JOIN q
      LEFT JOIN LATERAL (SELECT max(wl.entry_date) AS entry_date FROM work.thread_log wl
                          WHERE wl.thread_id = wt.id AND wt.updated_at = wt.parsed_at) wlast ON true
     CROSS JOIN LATERAL (SELECT concat_ws(E'\n', wt.title, wt.slug, wt.goal,
              wts.tldr, wts.where_we_are, wts.next_up, wts.blockers, wts.open_questions,
              wts.decisions) AS txt) txt
     CROSS JOIN LATERAL (SELECT txt.txt,
              COALESCE(wt.search_tsv, ''::tsvector) || COALESCE(wts.search_tsv, ''::tsvector) AS tsv) doc
     WHERE wt.project_id = 993478 AND (NULL::text IS NULL OR NULL::text = 'work_thread')
       AND ('fleet seat session work' = '' OR doc.tsv @@ q.match_tsq)
    UNION ALL
    SELECT COALESCE(doc.tsv @@ q.ident_tsq, false),
           'thread_entry:' || l.id, 'thread_entry',
           COALESCE(NULLIF(btrim(split_part(btrim(l.summary), E'\n', 1)), ''), l.kind),
           l.kind, NULL,
           CASE WHEN l.entry_date < (l.created_at AT TIME ZONE 'UTC')::date
                THEN l.entry_date::timestamp AT TIME ZONE 'UTC' ELSE l.created_at END,
           ts_rank_cd(doc.tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('slug', wt.slug, 'entry_kind', l.kind, 'entry_date', l.entry_date,
             'commit_sha', l.commit_sha, 'session_uuid', l.session_uuid),
           doc.txt, CASE WHEN l.session_uuid IS NOT NULL THEN jsonb_build_object('role', 'session',
             'session_uuid', l.session_uuid, 'session_title', ws.title) END,
           'thread_entry:' || l.id, NULL, NULL, false
      FROM (SELECT h.id FROM work.thread_log h
             WHERE 'fleet seat session work' <> '' AND NOT (SELECT pool FROM q) AND h.search_tsv @@ (SELECT match_tsq FROM q)
            UNION
            SELECT ph.k1::bigint FROM pool_pick ph WHERE ph.kind = 'thread_entry'
            UNION
            SELECT split_part(s.reference, ':', 2)::bigint FROM semantic s
             WHERE s.reference LIKE 'thread_entry:%'
            UNION
            (SELECT h.id FROM work.thread_log h JOIN work.threads ht ON ht.id = h.thread_id
             WHERE 'fleet seat session work' = '' AND ht.project_id = 993478
             ORDER BY h.created_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN work.thread_log l ON l.id = hit.id JOIN work.threads wt ON wt.id = l.thread_id
      LEFT JOIN sessions.sessions ws ON ws.session_uuid = l.session_uuid CROSS JOIN q
     CROSS JOIN LATERAL (SELECT concat_ws(E'\n\n', l.summary, l.body) AS txt, l.search_tsv AS tsv) doc
      LEFT JOIN semantic sem ON sem.reference = 'thread_entry:' || l.id
     WHERE wt.project_id = 993478 AND (NULL::text IS NULL OR NULL::text = 'thread_entry')
       AND ('fleet seat session work' = '' OR doc.tsv @@ q.match_tsq OR sem.score IS NOT NULL)
       AND (false OR l.kind IS DISTINCT FROM 'state-replaced')
    UNION ALL
    SELECT COALESCE(doc.tsv @@ q.ident_tsq, false),
           'question_answer:' || qa.question_uuid, 'question_answer',
           btrim(split_part(btrim(qa.question), E'\n', 1)), qa.state, NULL, qa.settled_at,
           ts_rank_cd(doc.tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('question_uuid', qa.question_uuid, 'run_id', qa.run_id,
             'to_task', qa.to_task, 'asker_session_uuid', asker.session_uuid),
           doc.txt, jsonb_build_object('role', 'session', 'session_uuid', settler.session_uuid,
             'session_title', settler.title),
           'question_answer:' || qa.question_uuid, NULL, NULL, false
      FROM (SELECT h.id FROM workspace.questions h
             WHERE 'fleet seat session work' <> '' AND NOT (SELECT pool FROM q) AND to_tsvector('english', h.question || ' ' || COALESCE(h.answer, ''))
                   @@ (SELECT match_tsq FROM q)
            UNION
            SELECT ph.k1::bigint FROM pool_pick ph WHERE ph.kind = 'question_answer'
            UNION
            SELECT h.id FROM workspace.questions h JOIN semantic s
                ON s.reference = 'question_answer:' || h.question_uuid
             WHERE s.reference LIKE 'question_answer:%'
            UNION
            SELECT h.id FROM workspace.questions h WHERE 'fleet seat session work' = '' AND h.project_id = 993478) hit
      JOIN workspace.questions qa ON qa.id = hit.id
      LEFT JOIN sessions.sessions asker ON asker.id = qa.asker_session_id
      LEFT JOIN sessions.sessions settler ON settler.id = qa.settled_by_session_id CROSS JOIN q
     CROSS JOIN LATERAL (SELECT 'Question: ' || qa.question || E'\n\nAnswer: ' || qa.answer AS txt,
              to_tsvector('english', qa.question || ' ' || COALESCE(qa.answer, '')) AS tsv OFFSET 0) doc
      LEFT JOIN semantic sem ON sem.reference = 'question_answer:' || qa.question_uuid
     WHERE qa.project_id = 993478 AND qa.answer IS NOT NULL AND (NULL::text IS NULL OR NULL::text = 'question_answer')
       AND ('fleet seat session work' = '' OR doc.tsv @@ q.match_tsq OR sem.score IS NOT NULL)
    UNION ALL
    SELECT COALESCE(vt.tsv @@ q.ident_tsq, false),
           vk.kind || ':' || v.id, vk.kind,
           CASE vk.kind WHEN 'unmet_ask' THEN btrim(split_part(btrim(v.unmet_ask), E'\n', 1))
                ELSE btrim(split_part(btrim(COALESCE(t.user_prompt, '')), E'\n', 1)) END,
           v.reaction, NULL, v.created_at,
           ts_rank_cd(vt.tsv, q.tsq),
           COALESCE(sem.score, 0),
           jsonb_build_object('verdict_id', v.id, 'turn_reference', 'turn:' || v.operator_turn_id,
             'reaction', v.reaction, 'session_uuid', vs.session_uuid),
           concat_ws(E'\n\n', NULLIF(btrim(COALESCE(v.unmet_ask, '')), ''), v.rationale),
           jsonb_build_object('role', 'judge', 'session_uuid', vs.session_uuid,
             'session_title', vs.title),
           vk.kind || ':' || v.id, v.operator_turn_id, t.user_prompt, false
      FROM (SELECT h.id FROM memory.turn_verdicts h
             WHERE 'fleet seat session work' <> '' AND NOT (SELECT pool FROM q) AND h.mode = 'live'
               AND to_tsvector('english', h.unmet_ask || ' ' || h.rationale) @@ (SELECT match_tsq FROM q)
            UNION
            SELECT ph.k1::bigint FROM pool_pick ph WHERE ph.kind = 'verdict'
            UNION
            SELECT split_part(s.reference, ':', 2)::bigint FROM semantic s
             WHERE s.reference LIKE 'unmet_ask:%' OR s.reference LIKE 'correction:%'
            UNION
            (SELECT h.id FROM memory.live_turn_verdicts_v h WHERE 'fleet seat session work' = '' AND h.project_id = 993478
             ORDER BY h.created_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN memory.live_turn_verdicts_v v ON v.id = hit.id
      JOIN memory.operator_turns t ON t.id = v.operator_turn_id
      LEFT JOIN sessions.sessions vs ON vs.id = v.session_id
     CROSS JOIN LATERAL (SELECT to_tsvector('english', v.unmet_ask || ' ' || v.rationale) AS tsv OFFSET 0) vt
     CROSS JOIN LATERAL (VALUES ('unmet_ask'), ('correction')) vk(kind) CROSS JOIN q
      LEFT JOIN semantic sem ON sem.reference = vk.kind || ':' || v.id
     WHERE v.project_id = 993478 AND (NULL::text IS NULL OR NULL::text IN ('unmet_ask', 'correction'))
       AND CASE vk.kind WHEN 'unmet_ask' THEN btrim(COALESCE(v.unmet_ask, '')) <> ''
                ELSE v.reaction IN ('corrected', 'redirected') END
       AND ('fleet seat session work' = '' OR vt.tsv @@ q.match_tsq
            OR sem.score IS NOT NULL)
    UNION ALL
    SELECT COALESCE(lk.tsv @@ q.ident_tsq, false),
           'learning:' || k.session_id, 'learning', btrim(split_part(btrim(k.learnings[1]), E'\n', 1)),
           'historical', NULL, k.generated_at,
           ts_rank_cd(lk.tsv, q.tsq),
           COALESCE(sem.score, 0),
           jsonb_build_object('session_uuid', k.session_id, 'topics', k.topics),
           lk.txt,
           jsonb_build_object('role', 'judge', 'session_uuid', k.session_id, 'session_title', ls.title),
           'learning:' || k.session_id, NULL, NULL, false
      FROM (SELECT h.session_id FROM memory.session_knowledge h
             WHERE 'fleet seat session work' <> '' AND NOT (SELECT pool FROM q) AND to_tsvector('english', memory.session_learnings_text(h.learnings))
                   @@ (SELECT match_tsq FROM q)
            UNION
            SELECT ph.k1::uuid FROM pool_pick ph WHERE ph.kind = 'learning'
            UNION
            SELECT h.session_id FROM memory.session_knowledge h JOIN semantic s
                ON s.reference = 'learning:' || h.session_id
             WHERE s.reference LIKE 'learning:%'
            UNION
            (SELECT h.session_id FROM memory.session_knowledge h
              JOIN sessions.sessions hs ON hs.session_uuid = h.session_id
             WHERE 'fleet seat session work' = '' AND hs.project_id = 993478 AND cardinality(h.learnings) > 0
             ORDER BY h.generated_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN memory.session_knowledge k ON k.session_id = hit.session_id
      JOIN sessions.sessions ls ON ls.session_uuid = k.session_id
     CROSS JOIN LATERAL (SELECT lkt.txt, to_tsvector('english', lkt.txt) AS tsv
                           FROM (SELECT memory.session_learnings_text(k.learnings) AS txt OFFSET 0) lkt
                         OFFSET 0) lk
     CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'learning:' || k.session_id
     WHERE ls.project_id = 993478 AND cardinality(k.learnings) > 0 AND (NULL::text IS NULL OR NULL::text = 'learning')
       AND ('fleet seat session work' = '' OR lk.tsv @@ q.match_tsq
            OR sem.score IS NOT NULL)
    UNION ALL
    SELECT COALESCE(sv.tsv @@ q.ident_tsq, false),
           'session:' || ss.session_uuid, 'session',
           COALESCE(NULLIF(btrim(ss.title), ''), left(regexp_replace(btrim(ss.overview), '\s+', ' ', 'g'), 80)),
           'historical', NULL, COALESCE(ss.overview_generated_at, ss.last_seen),
           ts_rank_cd(sv.tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('session_uuid', ss.session_uuid, 'topics', COALESCE(k.topics, '{}'::text[])),
           ss.overview || CASE WHEN cardinality(k.topics) > 0
                               THEN E'\n\nTopics: ' || array_to_string(k.topics, ', ') ELSE '' END,
           jsonb_build_object('role', 'judge', 'session_uuid', ss.session_uuid, 'session_title', ss.title),
           'session:' || ss.session_uuid, NULL, NULL, false
      FROM (SELECT h.id FROM sessions.sessions h
             WHERE 'fleet seat session work' <> '' AND NOT (SELECT pool FROM q) AND h.project_id = 993478
               AND to_tsvector('english', h.overview) @@ (SELECT match_tsq FROM q)
            UNION
            SELECT ph.k1::bigint FROM pool_pick ph WHERE ph.kind = 'session'
            UNION
            SELECT s0.id FROM semantic s2 JOIN sessions.sessions s0 ON s0.session_uuid = substr(s2.reference, 9)::uuid
             WHERE s2.reference LIKE 'session:%'
            UNION
            (SELECT h.id FROM sessions.sessions h
             WHERE 'fleet seat session work' = '' AND h.project_id = 993478 AND btrim(coalesce(h.overview, '')) <> ''
             ORDER BY COALESCE(h.overview_generated_at, h.last_seen) DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN sessions.sessions ss ON ss.id = hit.id
      LEFT JOIN memory.session_knowledge k ON k.session_id = ss.session_uuid
     CROSS JOIN LATERAL (SELECT to_tsvector('english', ss.overview) AS tsv OFFSET 0) sv
     CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'session:' || ss.session_uuid
     WHERE ss.project_id = 993478 AND btrim(coalesce(ss.overview, '')) <> '' AND (NULL::text IS NULL OR NULL::text = 'session')
       AND ('fleet seat session work' = '' OR sv.tsv @@ q.match_tsq OR sem.score IS NOT NULL)
    UNION ALL
    SELECT COALESCE(m.search_tsv @@ q.ident_tsq, false),
           'mail:' || m.message_uuid, 'mail', btrim(split_part(btrim(m.body), E'\n', 1)),
           CASE WHEN m.dead_lettered_at IS NOT NULL THEN 'dead_lettered'
                WHEN m.consumed_at IS NOT NULL THEN 'consumed'
                WHEN m.sent_at IS NOT NULL OR m.channel_delivered_at IS NOT NULL THEN 'delivered'
                ELSE 'queued' END,
           NULL, m.queued_at, ts_rank_cd(m.search_tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('message_uuid', m.message_uuid, 'thread_uuid', mt.thread_uuid,
             'to_session_uuid', mto.session_uuid, 'from_session_uuid', mf.session_uuid),
           m.body, jsonb_build_object('role', 'session', 'session_uuid', mf.session_uuid,
             'session_title', mf.title),
           'mail:' || m.message_uuid, NULL, NULL, false
      FROM (SELECT h.id FROM mailbox.messages h
             WHERE 'fleet seat session work' <> '' AND NOT (SELECT pool FROM q) AND h.from_session_id IS NOT NULL
               AND h.search_tsv @@ (SELECT match_tsq FROM q)
            UNION
            SELECT ph.k1::bigint FROM pool_pick ph WHERE ph.kind = 'mail'
            UNION
            SELECT h.id FROM mailbox.messages h JOIN semantic s
                ON s.reference = 'mail:' || h.message_uuid
             WHERE s.reference LIKE 'mail:%'
            UNION
            (SELECT h.id FROM mailbox.messages h WHERE 'fleet seat session work' = '' AND h.from_session_id IS NOT NULL
             ORDER BY h.queued_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN mailbox.messages m ON m.id = hit.id
      JOIN sessions.sessions mf ON mf.id = m.from_session_id
      LEFT JOIN mailbox.threads mt ON mt.id = m.thread_id
      LEFT JOIN sessions.sessions mto ON mto.id = m.to_session_id CROSS JOIN q
      LEFT JOIN semantic sem ON sem.reference = 'mail:' || m.message_uuid
     WHERE COALESCE(mt.project_id, mf.project_id) = 993478 AND (NULL::text IS NULL OR NULL::text = 'mail')
       AND ('fleet seat session work' = '' OR m.search_tsv @@ q.match_tsq OR sem.score IS NOT NULL)
    UNION ALL
    SELECT COALESCE(si.search_tsv @@ q.ident_tsq, false),
           seat_ref.reference, 'seat', btrim(split_part(btrim(si.text), E'\n', 1)), si.item_kind,
           NULL, si.at, ts_rank_cd(si.search_tsv, q.tsq), COALESCE(sem.score, 0),
           jsonb_build_object('tier', 'seat', 'item_kind', si.item_kind, 'run_id', si.run_id,
             'seat', si.seat, 'task', si.task, 'session_uuid', si.session_uuid,
             'from_session', si.from_session, 'from_role', si.from_role,
             'repeat_count', si.repeat_count, 'report_filed', si.report_filed,
             'verdict_accepted', si.verdict_accepted,
             'framing', 'seat material: written by or for a fleet seat or worker session, not by the operator'),
           si.text,
           jsonb_build_object('role', si.author, 'session_uuid', si.session_uuid,
             'session_title', sts.title, 'seat', si.seat, 'run_id', si.run_id, 'task', si.task),
           seat_ref.reference, NULL, NULL, false
      FROM (SELECT h.project_id, h.item_kind, h.source_ref, h.session_uuid FROM memory.seat_memory_items h
             WHERE 'fleet seat session work' <> '' AND NOT (SELECT pool FROM q) AND h.project_id = 993478 AND h.search_tsv @@ (SELECT match_tsq FROM q)
            UNION
            SELECT 993478::bigint, ph.k1, ph.k2, ph.k3::uuid FROM pool_pick ph WHERE ph.kind = 'seat'
            UNION
            SELECT h.project_id, h.item_kind, h.source_ref, h.session_uuid
              FROM semantic s
              JOIN memory.seat_memory_items h
                ON h.project_id = 993478
               AND h.source_ref IN (s.reference, CASE WHEN s.reference LIKE 'thread_entry:%'
                                                      THEN 'entry:' || substr(s.reference, 14) END)
             WHERE 'fleet seat session work' <> ''
               AND CASE WHEN h.source_ref LIKE 'entry:%' THEN 'thread_entry:' || substr(h.source_ref, 7)
                        ELSE h.source_ref END = s.reference
            UNION
            (SELECT h.project_id, h.item_kind, h.source_ref, h.session_uuid FROM memory.seat_memory_items h
              WHERE 'fleet seat session work' = '' AND h.project_id = 993478
              ORDER BY h.at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN memory.seat_memory_items si
        ON si.project_id = hit.project_id AND si.item_kind = hit.item_kind
       AND si.source_ref = hit.source_ref AND si.session_uuid = hit.session_uuid
      LEFT JOIN sessions.sessions sts ON sts.id = si.session_id CROSS JOIN q
     CROSS JOIN LATERAL (SELECT CASE WHEN si.source_ref LIKE 'entry:%'
                                     THEN 'thread_entry:' || substr(si.source_ref, 7)
                                     ELSE si.source_ref END AS reference) seat_ref
      LEFT JOIN semantic sem ON sem.reference = seat_ref.reference
     WHERE si.project_id = 993478 AND (NULL::text IS NULL OR NULL::text = 'seat') AND ('fleet seat session work' = '' OR si.search_tsv @@ q.match_tsq OR sem.score IS NOT NULL)
    UNION ALL
    SELECT COALESCE(kh.search_tsv @@ q.ident_tsq, false),
           kh.reference, kh.source_kind, kb.name, 'current', NULL::text, kh.changed_at,
           COALESCE(ts_rank_cd(kh.search_tsv, q.tsq), 0), COALESCE(sem.score, 0),
           jsonb_build_object('kind_label', kb.label, 'source_id', kh.source_id),
           kb.body, NULL::jsonb, kh.reference, NULL, NULL, false
      FROM kind_hits kh
      JOIN kind_bodies kb ON kb.kind = kh.source_kind AND kb.source_id = kh.source_id
     CROSS JOIN q
      LEFT JOIN semantic sem ON sem.reference = kh.reference
     WHERE kh.project_id = 993478 AND (NULL::text IS NULL OR NULL::text = kh.source_kind)
       AND ('fleet seat session work' = '' OR kh.search_tsv @@ q.match_tsq OR sem.score IS NOT NULL)
), cheap AS MATERIALIZED (
    SELECT c.*, row_number() OVER () AS rid
      FROM candidates c
      LEFT JOIN own_refs o ON o.reference = c.reference
     WHERE (NULL::text IS NULL OR c.source_kind = NULL)
       AND o.reference IS NULL
       AND (NULL::timestamptz IS NULL OR c.updated_at >= NULL::timestamptz)
       AND (NULL::timestamptz IS NULL OR c.updated_at < NULL::timestamptz)
       AND (NULL::text IS NULL
            OR (c.source_kind IN ('work_thread', 'thread_entry')
                AND c.provenance ->> 'slug' IN (SELECT slug FROM thread_scope))
            OR (c.source_kind = 'note' AND c.provenance -> 'tags' ? ('thread:' || NULL::text))
            OR COALESCE(c.author ->> 'session_uuid', c.provenance ->> 'session_uuid')
               IN (SELECT session_uuid FROM thread_scope_sessions))
), positioned AS (
    SELECT p.rid,
           row_number() OVER (ORDER BY p.rank DESC, p.reference) AS lex_pos,
           row_number() OVER (ORDER BY p.semantic_rank DESC, p.reference) AS sem_pos,
           row_number() OVER (PARTITION BY (p.rank > 0 OR p.semantic_rank > 0)
                              ORDER BY p.updated_at DESC NULLS LAST, p.reference) AS new_pos,
           (p.rank > 0) AS has_lex, (p.semantic_rank > 0) AS has_sem
      FROM cheap p
), bounded AS (
    SELECT c.*, p.lex_pos, p.sem_pos
      FROM cheap c
      JOIN positioned p ON p.rid = c.rid
     WHERE (p.has_lex AND p.lex_pos <= (SELECT m FROM bound))
        OR (p.has_sem AND p.sem_pos <= (SELECT m FROM bound))
        OR (NOT p.has_lex AND NOT p.has_sem AND p.new_pos <= (SELECT m FROM bound))
        OR c.is_ident
), eligible AS (
    SELECT c.*
      FROM bounded c
      LEFT JOIN LATERAL (
          SELECT nx.user_prompt AS next_prompt
            FROM memory.operator_turns cur
            JOIN memory.operator_turns nx ON nx.session_id = cur.session_id AND nx.id > cur.id
           WHERE cur.id = c.turn_id AND c.source_kind = 'turn' AND NOT c.answered AND NOT false
             AND btrim(COALESCE(c.turn_prompt, '')) <> ''
           ORDER BY nx.id LIMIT 1) resent ON true
     WHERE (resent.next_prompt IS NULL OR NOT starts_with(resent.next_prompt, btrim(c.turn_prompt)))
), deduped AS (
    SELECT DISTINCT ON (dedupe_key) *
      FROM (SELECT *, GREATEST(1, count(*) FILTER (WHERE source_kind <> 'seat')
                                    OVER (PARTITION BY dedupe_key)) AS copies
              FROM eligible) c
     ORDER BY dedupe_key, source_kind = 'seat' DESC, semantic_rank DESC, rank DESC,
              updated_at DESC NULLS LAST, reference
), keyed AS (
    SELECT d.*,
           CASE WHEN btrim(COALESCE(d.turn_prompt, '')) <> '' THEN 'p:' || md5(lower(btrim(d.turn_prompt)))
                WHEN d.source_kind IN ('artifact', 'memory', 'skill')
                    -- content_hash is the sha256 of the body, written with the revision. The blank test reads only
                    -- the first 64 characters unless they are all spaces.
                    THEN CASE WHEN kr.content_hash IS NOT NULL
                               AND CASE WHEN btrim(substr(kr.body, 1, 64)) <> '' THEN true
                                        ELSE btrim(kr.body) <> '' END
                              THEN 'b:' || kr.content_hash ELSE d.reference END
                WHEN btrim(COALESCE(d.body, '')) <> ''
                    THEN 'b:' || encode(sha256(convert_to(d.body, 'UTF8')), 'hex')
                ELSE d.reference END AS slot_key,
           d.source_kind IN ('seat', 'mail', 'thread', 'session')
           OR (d.source_kind = 'note' AND d.provenance ->> 'kind' = 'brief')
           OR EXISTS (SELECT 1 FROM memory.operator_turns mt WHERE mt.id = d.turn_id AND mt.opened_by = 'machine') AS machine,
           d.source_kind IN ('seat', 'thread')
           OR (d.source_kind = 'note' AND d.provenance ->> 'kind' = 'brief')
           OR EXISTS (SELECT 1 FROM memory.operator_turns mt WHERE mt.id = d.turn_id AND mt.opened_by = 'machine') AS ranked_lower
      FROM deduped d
      LEFT JOIN memory.knowledge_revisions kr
        ON kr.id = CASE WHEN d.source_kind IN ('artifact', 'memory', 'skill')
                        THEN substr(d.reference, 10)::bigint END
), slotted AS (
    SELECT k.*, row_number() OVER slot AS slot_pos,
           array_agg(k.reference) OVER (slot ROWS BETWEEN 1 FOLLOWING AND 10 FOLLOWING) AS also,
           count(*) OVER (PARTITION BY k.slot_key) - 1 AS also_count
      FROM keyed k
    WINDOW slot AS (PARTITION BY k.slot_key
                    ORDER BY k.semantic_rank DESC, k.rank DESC,
                             (k.source_kind = 'turn' AND k.answered) DESC,
                             k.updated_at DESC NULLS LAST, k.reference)
), winners AS (
    SELECT s.*, (rank > 0 AND lex_pos <= (SELECT m FROM bound)) AS in_lex,
           (semantic_rank > 0 AND sem_pos <= (SELECT m FROM bound)) AS in_sem
    FROM slotted s
    WHERE slot_pos = 1
), placed AS (
    -- A row kept because it is in one list but beyond M in the other is placed after the winners of that list,
    -- by its distance beyond M among all candidates, so that bounding the other list does not lift it.
    SELECT w.*,
           row_number() OVER (PARTITION BY in_lex ORDER BY rank DESC, reference) AS lex_rn,
           row_number() OVER (PARTITION BY in_sem ORDER BY semantic_rank DESC, reference) AS sem_rn,
           count(*) FILTER (WHERE in_lex) OVER () AS lex_w,
           count(*) FILTER (WHERE in_sem) OVER () AS sem_w
    FROM winners w
), ranked AS (
    SELECT p.*, ih.hits AS identifier_hits,
      (CASE WHEN rank > 0 THEN 1.0 / (60 + CASE WHEN in_lex THEN lex_rn
                       ELSE lex_w + GREATEST(lex_pos - (SELECT m FROM bound), 1) END) ELSE 0 END
      + CASE WHEN semantic_rank > 0 THEN 1.0 / (60 + CASE WHEN in_sem THEN sem_rn
                       ELSE sem_w + GREATEST(sem_pos - (SELECT m FROM bound), 1) END) ELSE 0 END)
      * CASE WHEN ranked_lower THEN 0.5 ELSE 1 END AS fused_rank
    FROM placed p
    CROSS JOIN LATERAL (
        SELECT count(*) AS hits FROM idents i
         WHERE COALESCE(p.title, '') ~* i.pattern OR COALESCE(p.body, '') ~* i.pattern) ih
), page AS (
    SELECT * FROM ranked
    ORDER BY identifier_hits DESC, fused_rank DESC, updated_at DESC, reference LIMIT 51 OFFSET 0
)
SELECT jsonb_build_object('reference' , p.reference, 'source_kind', p.source_kind,
    'title', p.title, 'status', p.status, 'path', p.path, 'updated_at', p.updated_at,
    'author', p.author, 'copies', p.copies,
    'also', COALESCE(to_jsonb(p.also), '[]'::jsonb), 'also_count', p.also_count,
    'excerpt', substr(COALESCE(p.body, ''),
                      GREATEST(1, CASE WHEN found.m IS NULL
                                       THEN COALESCE(NULLIF(strpos(lower(left(COALESCE(p.body, ''), 4000)),
                                                                   lower(split_part(btrim('fleet seat session work'), ' ', 1))), 0),
                                                     strpos(lower(COALESCE(p.body, '')),
                                                            lower(split_part(btrim('fleet seat session work'), ' ', 1))))
                                       ELSE length(COALESCE((found.m)[1], '')) + length(COALESCE((found.m)[2], '')) + 1
                                  END - 100), 300),
    'rank', p.fused_rank, 'lexical_rank', p.rank, 'semantic_rank', p.semantic_rank,
    'identifier_hits', p.identifier_hits,
    'provenance', p.provenance
                  || CASE WHEN p.machine THEN jsonb_build_object('written_by', 'machine') ELSE '{}'::jsonb END
                  || CASE WHEN p.ranked_lower THEN jsonb_build_object('ranked_lower', true) ELSE '{}'::jsonb END
    ) AS item
FROM page p
LEFT JOIN LATERAL (
    SELECT regexp_match(p.body, '^(.*?)' || i.pattern, 'i') AS m
      FROM idents i
     WHERE COALESCE(p.body, '') ~* i.pattern
     ORDER BY i.pos LIMIT 1) found ON true
ORDER BY p.identifier_hits DESC, p.fused_rank DESC, p.updated_at DESC, p.reference
