DELETE FROM memory.lesson_file_conflicts k
 USING memory.lesson_file_receipts r,
       jsonb_to_recordset('[{"lesson_uuid":"98a18513-b194-44b9-8267-a1bd8fd130ca","version":1},{"lesson_uuid":"e806a7d7-bff1-4fc6-bfa0-af5842e5545e","version":3}]'::jsonb) AS x(lesson_uuid uuid)
 WHERE k.location_uuid = '05ba052d-5a10-4d60-ad5f-dddb8821c3b3' AND k.lesson_uuid = x.lesson_uuid
   AND r.location_uuid = k.location_uuid AND r.lesson_uuid = k.lesson_uuid
   AND r.version >= k.server_version
