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
    SELECT u.reference, u.score FROM unnest(ARRAY['artifact:203920','artifact:203922','artifact:203923','artifact:203925','artifact:203930','artifact:204053','artifact:204054','artifact:204056','artifact:204066','artifact:204111','artifact:204112','artifact:204114','artifact:204133','artifact:204163','artifact:204164','artifact:204166','artifact:204185','correction:5739','correction:5746','learning:0c1403de-35c0-4965-a4e1-0323790d8fea','learning:1118336f-04ce-48d4-a4ac-feccfd112190','learning:a8097aca-6f5f-4b6a-a33a-d1aaa447eb78','lesson:1c48a912-79a2-4119-9f38-4bfaae8edfd9','lesson:30efac2b-d171-4c85-8673-989c4ad0970d','lesson:5b185abf-15fd-494e-a805-7808157be800','question_answer:0d8169f8-337a-4c1d-94c8-449d04710c6f','session:03863be4-4657-4270-8112-13eeab6f58ba','session:03e55f7d-0ac8-4a35-a776-94c0a56e605d','session:042f6575-a2f0-430f-92f4-6beba330db6d','session:04bde2d4-6cc6-4757-be1d-25a02d332775','session:085b7e06-1363-4c2d-8de5-7152ff2ab0a6','session:0874afe2-1920-47b3-943b-7751996a70b5','session:08e34a6a-5943-40ec-b4a9-680180fd4ccb','session:0aef5dec-90a3-431d-9088-c83b661d170d','session:0ba37dad-29ee-414c-a2b1-3ac978f24785','session:0c1403de-35c0-4965-a4e1-0323790d8fea','session:0cb2fbd3-c714-48e6-addb-05005d1e67df','session:1118336f-04ce-48d4-a4ac-feccfd112190','session:132cf506-e42d-4076-98f8-d9077fd3506f','session:132fb7d1-3e93-44fd-bdd0-a88b8ad53418','session:1595f4f2-427a-4646-8269-e234efe46183','session:16fb9aab-47e4-4a48-b591-863fddb4644a','session:18927ca8-f0ca-4ea6-bebc-69ebc32a558a','session:19efc8b6-34a9-4bcf-9b4d-b3bb8e4b1296','session:1bda3b3f-2dc5-403f-a6a7-717c057d7583','session:1c6eb135-9452-414d-94df-36fd1507244f','session:1cd1e207-f535-433e-a76b-13ef3b50899c','session:1eb5dc69-c3b0-4618-b2e2-274ba9c3f5a4','session:265e37c9-af4e-4ea7-b8eb-163458be8b56','session:2781798e-8941-4d11-ae35-d5ca24452394','session:27b1339f-49ab-46b4-8b9d-9658663bcf00','session:28214fa1-1c06-4ce0-b147-54aeb3b0916f','session:288442cd-47c8-41e5-b77f-d6aed1c8be78','session:2995d334-d0e4-4abf-9d91-04485456562b','session:29f720e2-ff2e-49b4-b7b8-b02e0a8bd714','session:2b3fc9f5-b36a-4f47-a026-a60aa07f51a3','session:2c6734f8-0276-4149-9f2e-02369f7dc49b','session:2d9de4d8-13cf-4195-9115-79be49564e82','session:3120267c-5529-4179-9420-f2c6406a348f','session:31779c95-51a1-42ee-a339-ee289d998e2c','session:32ab235c-ff26-4625-b491-db2a91c0434c','session:33160e88-3215-4f19-88d3-4367ec0457ed','session:33d335cd-454b-40af-8602-26211dc03858','session:34c8383b-77c6-438f-b49a-08a49d1010a7','session:3d826259-d0a5-4647-a035-229aad4cfa0b','session:3ef2d49e-845d-4d9e-b574-83b0553cd717','session:3f1dbb3e-ab58-4922-bdae-b00a810a278f','session:40049bd2-078a-4a25-8e3f-d918d9cc7257','session:402f408c-c942-443c-94f8-847d0f5d4830','session:411db949-338d-447f-ba36-cd3a2e69e2e2','session:43cadf4e-036c-41ca-9f1e-9b0b53ea9753','session:441571ac-8bc0-4d9d-b953-41acbd1256c7','session:45ff215f-5d16-413d-9497-2dffe0c4367f','session:4748e283-7765-4f8d-9c5f-5e0c5622c040','session:488d82f0-691c-4f17-8a6b-836eb3e44a3d','session:4917e99e-4c38-489e-bfa1-874611d3a907','session:4d719c1e-cbd6-4385-b67d-7494f4828473','session:4dc318a3-b1a9-483f-9b33-c22981ba89b9','session:5162637f-a725-4d8b-beee-0e1f1dba4787','session:52716551-bf1b-4f9a-a694-5bb2b7c6adff','session:54523ebb-c4ed-4029-895c-98640b6b2c91','session:55edee43-7a2c-46ee-863a-905807320d98','session:5755d0c9-506f-45e0-a4ed-c93794caf1ba','session:5b96e68a-a9fa-4787-a864-60b16023e40f','session:5e49af53-8fbd-445a-8d19-cee3666a191f','session:6199678b-cfa7-4453-b8be-fb263023a0c7','session:61f21350-225c-4a9c-bd48-f6702f0c22af','session:64c68892-ee8e-47f7-b990-e68ad7d5551f','session:650be2c5-4778-4c3d-94be-5507f3001516','session:6536926a-7d13-4d70-9365-b0f121a51685','session:656ac3c9-99de-4725-b527-500555dd29c2','session:658bf649-5f7a-4287-ae09-a46b26f65486','session:6636b0ff-b7bb-4367-ac56-05c5ffed4dfa','session:66e279b6-fd2c-47c2-be99-835c9e580daa','session:685cda67-9db5-4147-a7c3-fc74ab358a21','session:6b7d121c-abaf-457a-bf16-e16cae50cd96','session:6bac5545-a39b-4dbe-9d9d-920c9ad08dc5','session:6bf89d66-0869-4fe6-a489-0c0e318139ff','session:6cd3b15f-192f-4582-ae20-21e23c99fdfd','session:6e160a2f-139d-403a-ab5e-7ab776dbfded','session:765db08e-8ef9-4d6c-a6ac-a8f2cd38c738','session:767d1dec-d573-443b-874d-d52141a8615b','session:778d005e-dbe8-4f7c-8686-3fe656fabae0','session:788173b9-3dbd-41c1-b2d9-474a81a0ef01','session:79c3a6ab-6e50-4d75-9986-cf25bac9031b','session:7ba3bfa1-d026-40f0-bf6b-3c336d7c795b','session:7fd50d18-7afa-422c-a6f4-ee4746be87d8','session:857b33d6-db19-4eed-88ec-a73682fa22e2','session:8881f1e3-a560-4a73-8319-51690d86370c','session:8b064c5d-c5ed-46f3-ad32-8761856dee3e','session:8f0e0699-8dbf-4649-87d9-57fc8ef34ce2','session:8fcc5ac1-beac-4ff1-8c19-282685fb7e6b','session:9223c334-8dcd-4b98-bd0d-ed058bd3096e','session:95783c57-73d7-43fb-909e-6270068857c3','session:9597b7cd-7e8a-4a6d-a304-da684e51aa8e','session:972db9e3-cad8-4c85-a65f-0edbb4fb8281','session:980aa22f-0e1f-49eb-bc4d-eb99268ed165','session:9818579d-6eac-4ff9-a07f-b205a4920599','session:998251e3-bf24-45b4-b795-d7369754367e','session:9af4857d-e4e2-4bcb-9c7d-16329d7dddda','session:a3925b22-de52-42cf-a6eb-ccfa61af9948','session:a4609dac-cf8d-424d-82fe-a86460e2ea9d','session:a54b2361-93bf-44d8-9322-a589aaff66ee','session:a770c762-8e22-4cc5-840e-490228fb4d38','session:a8097aca-6f5f-4b6a-a33a-d1aaa447eb78','session:adcc528f-1922-440c-bb32-876f7ce383b3','session:b3a7a919-300c-4fc1-8516-54ead9222bed','session:b8a3128f-0e91-4f8d-9a85-8a7dce79cfef','session:bfc39679-fe80-48a6-bd97-6c7c2b59aee7','session:c106441d-ce23-4d05-af91-c3c945ef0e45','session:c144304f-c9c8-439a-9045-c6b4da772a4e','session:c242a8fb-1894-42e7-930e-c6bf95f93c5d','session:c2d62b93-c06f-4c75-85d8-70de8622f41d','session:c48949e0-7e28-4a94-bbe6-f1fb9a16274c','session:c530a664-00b9-48a0-bda6-c6ac5dcb47b4','session:c5c2d3af-af24-49fe-b84d-b38dce6c7682','session:c713680f-3426-4677-a004-4afd83eb09d9','session:c8d20401-57fb-4034-babe-5e6e8d1918e4','session:c9679434-5d90-4510-ad2e-eeca54b2998b','session:d07a9240-bbbc-4a78-b7ab-c6102c76a833','session:d78d1911-3140-40c6-bd2b-6b88814b8966','session:dd877f03-707e-40f8-bf7e-633b745bdc68','session:e1354e08-2eb6-4533-8ec5-350efc3c4ae0','session:e426a566-853e-4719-b9fc-302c0f008f51','session:e520efc0-3f00-4f92-86a4-10058b05ab97','session:e93425a0-0997-4bb0-a546-f46b46b2f0c9','session:eac3fc9d-c4d1-4e03-86fe-445efaeb53b0','session:ec0d7b12-2fc7-4441-82c3-60baf247c550','session:ec5a4bd7-71a1-4378-b924-4cbb32f6206b','session:ecfbb1d8-37cd-41ae-a33d-8d7c3b4530ee','session:eeb787eb-fc86-41a0-a137-fbdd73bd983b','session:f7a2b0b2-eef8-4548-abba-94dcdb4d61bb','session:fbd430e6-c546-4dbe-ac87-a3cb36df5546','session:fc5102b6-9a3d-4053-85e4-0eb016214997','session:fe4ca544-5ca0-4b39-b1fa-ca99e27fce70','turn:12923','turn:13010','turn:13702','turn:13704','turn:14406','turn:14458','turn:14818','turn:14908','turn:14923','turn:15735','turn:15824','turn:18813','turn:18814','turn:18815','turn:19499','turn:19512','turn:19720','turn:19721','turn:19733','turn:19736','turn:22746','turn:22894','turn:24126','turn:26010','turn:26073','turn:26270','turn:26281','turn:26361','turn:27107','turn:27109','turn:28965','turn:30642','turn:36686','turn:39752','turn:39759','turn:39768','turn:41690','turn:41696','turn:45243','turn:51045','turn:52751','turn:52752','turn:52755','turn:52756','turn:52759','turn:52761','turn:52762','turn:52763','turn:52765','turn:52766','turn:52773','turn:52778','turn:52781','turn:52786','turn:52787','turn:52788','turn:52789','turn:52792','turn:52794','turn:52796','turn:52798','turn:52800','turn:52801','turn:52806','turn:52808','turn:52815','turn:52817','turn:52827','turn:52833','turn:52838','turn:52840','turn:52842','turn:52843','turn:52846','turn:52851','turn:52854','turn:52855','turn:52862','turn:52864','turn:52866','turn:52870','turn:52885','turn:52891','turn:52893','turn:52895','turn:52896','turn:52903','turn:52904','turn:52916','turn:52918','turn:52928','turn:52930','turn:52944','turn:52947','turn:52953','turn:52956','turn:52959','turn:52960','turn:52963','turn:52964','turn:52979','turn:53047','turn:53058','turn:53063','turn:53066','turn:53069','turn:53077','turn:53082','turn:53085','turn:53086','turn:53090','turn:53099','turn:53100','turn:53118','turn:53126','turn:53127','turn:53136','turn:53154','turn:53159','turn:53162','turn:53173','turn:53176','turn:53178','turn:53181','turn:53189','turn:53199','turn:53217','turn:53223','turn:53224','turn:53260','turn:53273','turn:53293','turn:53299','turn:53303','turn:53319','turn:53341','turn:53354','turn:53357','turn:53362','turn:53364','turn:55414','turn:55662','unmet_ask:14213','unmet_ask:14339','unmet_ask:14459','unmet_ask:14461','unmet_ask:5579','unmet_ask:5746']::text[]::text[], ARRAY[0.4672230264711834,0.5577049891577648,0.5207756391192471,0.5509862139527688,0.5481313687758894,0.544449020425917,0.4914626322504516,0.5203491318538288,0.6027710813571323,0.544449020425917,0.4914626322504516,0.5203491318538288,0.6027710813571323,0.544449020425917,0.4914626322504516,0.5203491318538288,0.6027710813571323,0.4859355501359741,0.4816008607866742,0.47942532047116937,0.529118346791148,0.5380395351205458,0.486064716758309,0.46748862288587634,0.47210068584990506,0.5508083062070175,0.47011976543658274,0.5444143848296237,0.4834078982539346,0.4685476803730162,0.4858971391351369,0.46934374435991666,0.47140513786604543,0.5528191759342753,0.5226923410136204,0.5180044260046138,0.5460337619942494,0.5055750457771614,0.5122021284043666,0.4889074226286739,0.4939067776271391,0.5020472561843841,0.5142097932884189,0.4675038971744714,0.5344816451437677,0.5032289478089735,0.46908716581053567,0.5555591550145892,0.4769503006592918,0.520691656142221,0.51161840904308,0.5161464964195429,0.5161962720916136,0.4713630748470976,0.5683239529036865,0.5009174794779224,0.5604465508860548,0.4818982316760887,0.5944081947110288,0.4725117319850358,0.47472403265584917,0.5255894909102735,0.5727301838729294,0.5136792539285169,0.48690479681607,0.5011812903633798,0.49855400617585366,0.5483980457658355,0.48451678501024364,0.5042626432318986,0.530636134209834,0.5109735322790124,0.533867168474271,0.4766656586527179,0.5597317456799568,0.4997573494043417,0.47187508726496974,0.5106079730573075,0.5559139815389597,0.5649293310835164,0.5377311546287133,0.4680038214925619,0.5340379954014703,0.5236562334402061,0.5837626979139193,0.4668091079278516,0.5688038298844718,0.48423821801376765,0.5303812744873303,0.47114554591778124,0.5155675005944877,0.47531670571518125,0.5085995652019862,0.4687925642470494,0.560515692280176,0.4989145392964083,0.5874612856622016,0.5756709936480502,0.5267284169038959,0.5100748362937717,0.46590063551383887,0.4683939339274956,0.5075296599374541,0.508222955670906,0.48872257668282426,0.509796787707562,0.5723614180957738,0.5205288447683264,0.48942096436809823,0.49751286112270354,0.4921369671160156,0.5040674519739938,0.47362576032070436,0.5048405076706604,0.5065463638198378,0.5081846318208467,0.5258737676627114,0.5282819743408822,0.5277826065823069,0.5889056957465499,0.4679448705829655,0.4714660401978048,0.5394741920266416,0.5805750258319213,0.5407991619109053,0.5213863640847534,0.567352941693659,0.467033630707945,0.489765510268096,0.47334855337363324,0.47570795080170714,0.5243725649451805,0.4760079156186323,0.4740177472281426,0.4757653333019096,0.5211664488712511,0.5995339809876176,0.4800599014919785,0.5532596935718994,0.4661975283438873,0.5341292819573598,0.5070042341369175,0.4891941680303642,0.48526189562465083,0.470043045584261,0.4834748660482989,0.5649866525785104,0.5551159216429573,0.5274813018000265,0.47094831408356186,0.4790744576355903,0.4922176136232512,0.4963708663539568,0.5041870630066372,0.46636960097565705,0.4696777548713871,0.5244083022974406,0.5064325181713456,0.5313356230469246,0.5251044504438132,0.46957056626221005,0.4769090514887484,0.5496160729232119,0.5007760377357193,0.522041548916436,0.6255904885258049,0.47906966445061183,0.48027621385971986,0.4977236829686338,0.47427429982254243,0.5722792574084599,0.48982949524613406,0.5994935808905801,0.5157178561618283,0.46920458697124146,0.5033789143170035,0.5173251824357162,0.48718864948513807,0.5351361598520824,0.5325392364745936,0.5502334937245217,0.5036468377497824,0.46751135431805513,0.48852459846951213,0.4738536257947328,0.4776575627788402,0.4896048447136705,0.47230111727152124,0.5478564721132225,0.524778532217042,0.5022250269524918,0.4915211347199193,0.542699021905373,0.592984976790687,0.522145232537606,0.5074378100801674,0.47907818043280703,0.46966961341217106,0.4669843595842771,0.4943407606407533,0.47086356319759903,0.483154550696822,0.4673985443706976,0.46698150840016894,0.4835700209435969,0.470637028724175,0.4824794962239074,0.5152052794458442,0.5037458693602668,0.48620800715069845,0.5210580149781178,0.5095017411859674,0.5076788237425567,0.4703334096372449,0.466995658604609,0.4836654201447109,0.48060960316434054,0.47535598519598876,0.5095326198038251,0.4944900107201988,0.5143697091386981,0.49691997385859343,0.5042580215854546,0.5237301076877512,0.4775306346386152,0.5720426865803966,0.4956590686149873,0.5039643208884917,0.48433057169973204,0.5792154747949859,0.4825796895640546,0.5044056032066857,0.4740141786929193,0.4933959090765898,0.604420882331763,0.48951820685566494,0.4958454816295722,0.47196646285362975,0.47718055070233945,0.5160667203883527,0.46756798262547794,0.4700252059275003,0.47218026801172575,0.548327615159351,0.6121115111877172,0.49094517286141925,0.5233999390542494,0.46971440993040403,0.4861943576874517,0.5172284995683969,0.4717808860948879,0.5461581026143768,0.48900650081251784,0.4903532103230549,0.4738257652961657,0.47380660396241536,0.5074378100801674,0.4899532917016687,0.483154550696822,0.49426522611716883,0.48328060654428917,0.4994528144109054,0.5103565643619103,0.5079191487907277,0.5568180121480885,0.5030212852080096,0.4693414392478813,0.47807280523238416,0.483117234165102,0.47998704790605906,0.5116457384749917,0.4807902110329718,0.5042580215854546,0.5022569149060478,0.4738280631951548,0.5015094127183926,0.5965924315496521,0.4711981699694814,0.499547526361829,0.5792154747949859,0.473206394483191,0.5091891744008196,0.48275227484900585,0.5719325627149574,0.5029862084053773,0.4700252059275003,0.5732119937557179,0.6121115111877172,0.48146580656801274,0.4935258174114925,0.5106093110001154,0.5038485695548579,0.5461581026143768,0.4688805164886234,0.46790165967434594,0.515652998815439,0.5675009641800663,0.47141044139105615,0.519325691361102,0.5289377235230927,0.5597942518513455,0.4945291123582768,0.4666606207665228]::float8[]::float8[]) AS u(reference, score)
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
                          WHERE l0.project_id = 1 OR s0.machine_uuid IS NOT NULL) h
    UNION ALL
    SELECT 'lesson', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT l0.id FROM memory.lessons l0
                          WHERE l0.project_id = 1 AND to_tsvector('english', l0.text) @@ t.tq) h
    UNION ALL
    SELECT 'note', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT n0.id FROM workspace.session_notes n0 JOIN sessions.sessions s0 ON s0.id = n0.session_id
                          WHERE n0.search_tsv @@ t.tq AND s0.project_id = 1) h
    UNION ALL
    SELECT 'turn', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT t0.id FROM memory.operator_turns t0
                          WHERE t0.project_id = 1 AND t0.search_tsv @@ t.tq
                            AND (t0.opened_by IS NULL OR t0.opened_by = 'operator')) h
    UNION ALL
    SELECT 'thread_msg', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT m0.id FROM mailbox.messages m0 JOIN mailbox.threads p0 ON p0.id = m0.thread_id
                          WHERE m0.search_tsv @@ t.tq AND p0.project_id = 1) h
    UNION ALL
    SELECT 'thread_entry', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT e0.id FROM work.thread_log e0 JOIN work.threads w0 ON w0.id = e0.thread_id
                          WHERE e0.search_tsv @@ t.tq AND w0.project_id = 1) h
    UNION ALL
    SELECT 'question_answer', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT a0.id FROM workspace.questions a0
                          WHERE a0.project_id = 1
                            AND to_tsvector('english', a0.question || ' ' || COALESCE(a0.answer, '')) @@ t.tq) h
    UNION ALL
    SELECT 'verdict', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT v0.id FROM memory.turn_verdicts v0 JOIN memory.operator_turns vt0 ON vt0.id = v0.operator_turn_id
                          WHERE v0.mode = 'live' AND vt0.project_id = 1
                            AND to_tsvector('english', v0.unmet_ask || ' ' || v0.rationale) @@ t.tq) h
    UNION ALL
    SELECT 'learning', h.session_id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT k0.session_id FROM memory.session_knowledge k0
                           JOIN sessions.sessions s0 ON s0.session_uuid = k0.session_id
                          WHERE s0.project_id = 1
                            AND to_tsvector('english', memory.session_learnings_text(k0.learnings)) @@ t.tq) h
    UNION ALL
    SELECT 'session', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT s0.id FROM sessions.sessions s0
                          WHERE s0.project_id = 1 AND to_tsvector('english', s0.overview) @@ t.tq) h
    UNION ALL
    SELECT 'mail', h.id::text, NULL, NULL, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT m0.id FROM mailbox.messages m0
                           JOIN sessions.sessions f0 ON f0.id = m0.from_session_id
                           LEFT JOIN mailbox.threads p0 ON p0.id = m0.thread_id
                          WHERE m0.from_session_id IS NOT NULL AND m0.search_tsv @@ t.tq
                            AND COALESCE(p0.project_id, f0.project_id) = 1) h
    UNION ALL
    SELECT 'seat', h.item_kind, h.source_ref, h.session_uuid::text, t.ti
      FROM terms t
     CROSS JOIN LATERAL (SELECT i0.item_kind, i0.source_ref, i0.session_uuid FROM memory.seat_memory_items i0
                          WHERE i0.project_id = 1 AND i0.search_tsv @@ t.tq) h
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
    SELECT wt.id AS thread_id, wt.slug FROM work.threads wt WHERE wt.project_id = 1 AND wt.slug = NULL::text
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
     WHERE sl.project_id = 1 OR s.machine_uuid IS NOT NULL
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
     WHERE 'fleet seat session work' <> '' AND h.project_id = 1 AND h.completed_at IS NOT NULL
       AND h.search_tsv @@ (SELECT match_tsq FROM q)
       AND (NULL::text IS NULL OR h.source_kind = NULL::text)
    UNION
    SELECT h.id
      FROM memory.knowledge_kinds nk
      JOIN semantic s2 ON starts_with(s2.reference, nk.kind || ':')
      JOIN memory.knowledge_embedding_state h ON h.reference = s2.reference AND h.source_kind = nk.kind
     WHERE NOT nk.built_in AND h.project_id = 1 AND h.completed_at IS NOT NULL
       AND (NULL::text IS NULL OR h.source_kind = NULL::text)
    UNION
    (SELECT newest.state_id
       FROM memory.knowledge_kinds nk
      CROSS JOIN LATERAL (
          SELECT e.id AS state_id, hi.changed_at
            FROM memory.knowledge_items hi
            JOIN memory.knowledge_embedding_state e ON e.item_id = hi.id AND e.completed_at IS NOT NULL
           WHERE 'fleet seat session work' = '' AND hi.kind = nk.kind AND hi.project_id = 1
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
             WHERE 'fleet seat session work' = '' AND (l0.project_id = 1 OR s0.machine_uuid IS NOT NULL)
             ORDER BY r0.observed_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN memory.knowledge_revisions r ON r.id = hit.id
      JOIN memory.knowledge_documents d ON d.document_uuid = r.document_uuid AND d.current_revision = r.id
      JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
      LEFT JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
      JOIN source_freshness sf ON sf.source_uuid = s.source_uuid
      CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'artifact:' || r.id
     WHERE (loc.project_id = 1 OR s.machine_uuid IS NOT NULL)
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
             WHERE 'fleet seat session work' = '' AND (l0.project_id = 1 OR s0.machine_uuid IS NOT NULL)
             ORDER BY r0.observed_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN memory.knowledge_revisions r ON r.id = hit.id
      JOIN memory.knowledge_documents d ON d.document_uuid = r.document_uuid AND d.current_revision = r.id
      JOIN memory.knowledge_sources s ON s.source_uuid = d.source_uuid
      LEFT JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
      JOIN source_freshness sf ON sf.source_uuid = s.source_uuid
      CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'artifact:' || r.id
     WHERE (loc.project_id = 1 OR s.machine_uuid IS NOT NULL)
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
             WHERE 'fleet seat session work' <> '' AND NOT (SELECT pool FROM q) AND h.project_id = 1
               AND to_tsvector('english', h.text) @@ (SELECT match_tsq FROM q)
            UNION
            SELECT ph.k1::bigint FROM pool_pick ph WHERE ph.kind = 'lesson'
            UNION
            SELECT l0.id FROM semantic s2 JOIN memory.lessons l0 ON l0.uuid = substr(s2.reference, 8)::uuid
             WHERE s2.reference LIKE 'lesson:%'
            UNION
            SELECT h.id FROM memory.lessons h WHERE 'fleet seat session work' = '' AND h.project_id = 1) hit
      JOIN memory.lessons l ON l.id = hit.id
      JOIN memory.lesson_kinds k ON k.id = l.kind_id
      JOIN memory.lesson_stages st ON st.id = l.stage_id
      LEFT JOIN lesson_marks lm ON lm.to_lesson_id = l.id
      LEFT JOIN memory.lessons rby ON rby.id = l.retired_by_lesson_id
      LEFT JOIN memory.operator_turns lst ON lst.id = l.source_turn_id
      LEFT JOIN sessions.sessions lss ON lss.id = COALESCE(l.session_id, lst.session_id)
     CROSS JOIN LATERAL (SELECT to_tsvector('english', l.text) AS tsv OFFSET 0) lt
      CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'lesson:' || l.uuid
     WHERE l.project_id = 1 AND (NULL::text IS NULL OR NULL::text = 'lesson')
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
             WHERE 'fleet seat session work' = '' AND hs.project_id = 1
             ORDER BY h.updated_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN workspace.session_notes n ON n.id = hit.id
      JOIN sessions.sessions ss ON ss.id = n.session_id CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'note:' || n.note_uuid
     WHERE ss.project_id = 1 AND (NULL::text IS NULL OR NULL::text = 'note')
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
             WHERE 'fleet seat session work' <> '' AND NOT (SELECT pool FROM q) AND h.project_id = 1 AND h.search_tsv @@ (SELECT match_tsq FROM q)
            UNION
            SELECT ph.k1::bigint FROM pool_pick ph WHERE ph.kind = 'turn'
            UNION
            SELECT split_part(s.reference, ':', 2)::bigint FROM semantic s
             WHERE s.reference LIKE 'turn:%'
            UNION
            (SELECT h.id FROM memory.operator_turns h WHERE 'fleet seat session work' = '' AND h.project_id = 1
             ORDER BY h.created_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN memory.operator_turns t ON t.id = hit.id
      JOIN sessions.sessions ss ON ss.id = t.session_id CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'turn:' || t.id
     WHERE t.project_id = 1 AND (NULL::text IS NULL OR NULL::text = 'turn')
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
                  WHERE 'fleet seat session work' <> '' AND pt.project_id = 1 AND m.search_tsv @@ q.match_tsq
                    AND (NOT q.pool OR m.id IN (SELECT ph.k1::bigint FROM pool_pick ph WHERE ph.kind = 'thread_msg'))
                  GROUP BY m.thread_id) hit ON hit.thread_id = th.id
      LEFT JOIN semantic sem ON sem.reference = 'thread:' || th.thread_uuid
     WHERE th.project_id = 1 AND (NULL::text IS NULL OR NULL::text = 'thread')
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
     WHERE wt.project_id = 1 AND (NULL::text IS NULL OR NULL::text = 'work_thread')
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
             WHERE 'fleet seat session work' = '' AND ht.project_id = 1
             ORDER BY h.created_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN work.thread_log l ON l.id = hit.id JOIN work.threads wt ON wt.id = l.thread_id
      LEFT JOIN sessions.sessions ws ON ws.session_uuid = l.session_uuid CROSS JOIN q
     CROSS JOIN LATERAL (SELECT concat_ws(E'\n\n', l.summary, l.body) AS txt, l.search_tsv AS tsv) doc
      LEFT JOIN semantic sem ON sem.reference = 'thread_entry:' || l.id
     WHERE wt.project_id = 1 AND (NULL::text IS NULL OR NULL::text = 'thread_entry')
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
            SELECT h.id FROM workspace.questions h WHERE 'fleet seat session work' = '' AND h.project_id = 1) hit
      JOIN workspace.questions qa ON qa.id = hit.id
      LEFT JOIN sessions.sessions asker ON asker.id = qa.asker_session_id
      LEFT JOIN sessions.sessions settler ON settler.id = qa.settled_by_session_id CROSS JOIN q
     CROSS JOIN LATERAL (SELECT 'Question: ' || qa.question || E'\n\nAnswer: ' || qa.answer AS txt,
              to_tsvector('english', qa.question || ' ' || COALESCE(qa.answer, '')) AS tsv OFFSET 0) doc
      LEFT JOIN semantic sem ON sem.reference = 'question_answer:' || qa.question_uuid
     WHERE qa.project_id = 1 AND qa.answer IS NOT NULL AND (NULL::text IS NULL OR NULL::text = 'question_answer')
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
            (SELECT h.id FROM memory.live_turn_verdicts_v h WHERE 'fleet seat session work' = '' AND h.project_id = 1
             ORDER BY h.created_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN memory.live_turn_verdicts_v v ON v.id = hit.id
      JOIN memory.operator_turns t ON t.id = v.operator_turn_id
      LEFT JOIN sessions.sessions vs ON vs.id = v.session_id
     CROSS JOIN LATERAL (SELECT to_tsvector('english', v.unmet_ask || ' ' || v.rationale) AS tsv OFFSET 0) vt
     CROSS JOIN LATERAL (VALUES ('unmet_ask'), ('correction')) vk(kind) CROSS JOIN q
      LEFT JOIN semantic sem ON sem.reference = vk.kind || ':' || v.id
     WHERE v.project_id = 1 AND (NULL::text IS NULL OR NULL::text IN ('unmet_ask', 'correction'))
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
             WHERE 'fleet seat session work' = '' AND hs.project_id = 1 AND cardinality(h.learnings) > 0
             ORDER BY h.generated_at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN memory.session_knowledge k ON k.session_id = hit.session_id
      JOIN sessions.sessions ls ON ls.session_uuid = k.session_id
     CROSS JOIN LATERAL (SELECT lkt.txt, to_tsvector('english', lkt.txt) AS tsv
                           FROM (SELECT memory.session_learnings_text(k.learnings) AS txt OFFSET 0) lkt
                         OFFSET 0) lk
     CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'learning:' || k.session_id
     WHERE ls.project_id = 1 AND cardinality(k.learnings) > 0 AND (NULL::text IS NULL OR NULL::text = 'learning')
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
             WHERE 'fleet seat session work' <> '' AND NOT (SELECT pool FROM q) AND h.project_id = 1
               AND to_tsvector('english', h.overview) @@ (SELECT match_tsq FROM q)
            UNION
            SELECT ph.k1::bigint FROM pool_pick ph WHERE ph.kind = 'session'
            UNION
            SELECT s0.id FROM semantic s2 JOIN sessions.sessions s0 ON s0.session_uuid = substr(s2.reference, 9)::uuid
             WHERE s2.reference LIKE 'session:%'
            UNION
            (SELECT h.id FROM sessions.sessions h
             WHERE 'fleet seat session work' = '' AND h.project_id = 1 AND btrim(coalesce(h.overview, '')) <> ''
             ORDER BY COALESCE(h.overview_generated_at, h.last_seen) DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN sessions.sessions ss ON ss.id = hit.id
      LEFT JOIN memory.session_knowledge k ON k.session_id = ss.session_uuid
     CROSS JOIN LATERAL (SELECT to_tsvector('english', ss.overview) AS tsv OFFSET 0) sv
     CROSS JOIN q LEFT JOIN semantic sem ON sem.reference = 'session:' || ss.session_uuid
     WHERE ss.project_id = 1 AND btrim(coalesce(ss.overview, '')) <> '' AND (NULL::text IS NULL OR NULL::text = 'session')
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
     WHERE COALESCE(mt.project_id, mf.project_id) = 1 AND (NULL::text IS NULL OR NULL::text = 'mail')
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
             WHERE 'fleet seat session work' <> '' AND NOT (SELECT pool FROM q) AND h.project_id = 1 AND h.search_tsv @@ (SELECT match_tsq FROM q)
            UNION
            SELECT 1::bigint, ph.k1, ph.k2, ph.k3::uuid FROM pool_pick ph WHERE ph.kind = 'seat'
            UNION
            SELECT h.project_id, h.item_kind, h.source_ref, h.session_uuid
              FROM semantic s
              JOIN memory.seat_memory_items h
                ON h.project_id = 1
               AND h.source_ref IN (s.reference, CASE WHEN s.reference LIKE 'thread_entry:%'
                                                      THEN 'entry:' || substr(s.reference, 14) END)
             WHERE 'fleet seat session work' <> ''
               AND CASE WHEN h.source_ref LIKE 'entry:%' THEN 'thread_entry:' || substr(h.source_ref, 7)
                        ELSE h.source_ref END = s.reference
            UNION
            (SELECT h.project_id, h.item_kind, h.source_ref, h.session_uuid FROM memory.seat_memory_items h
              WHERE 'fleet seat session work' = '' AND h.project_id = 1
              ORDER BY h.at DESC NULLS LAST LIMIT (SELECT m FROM bound))) hit
      JOIN memory.seat_memory_items si
        ON si.project_id = hit.project_id AND si.item_kind = hit.item_kind
       AND si.source_ref = hit.source_ref AND si.session_uuid = hit.session_uuid
      LEFT JOIN sessions.sessions sts ON sts.id = si.session_id CROSS JOIN q
     CROSS JOIN LATERAL (SELECT CASE WHEN si.source_ref LIKE 'entry:%'
                                     THEN 'thread_entry:' || substr(si.source_ref, 7)
                                     ELSE si.source_ref END AS reference) seat_ref
      LEFT JOIN semantic sem ON sem.reference = seat_ref.reference
     WHERE si.project_id = 1 AND (NULL::text IS NULL OR NULL::text = 'seat') AND ('fleet seat session work' = '' OR si.search_tsv @@ q.match_tsq OR sem.score IS NOT NULL)
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
     WHERE kh.project_id = 1 AND (NULL::text IS NULL OR NULL::text = kh.source_kind)
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
