SELECT d.id AS "id!"
  FROM memory.knowledge_consolidation_decisions d
  LEFT JOIN memory.knowledge_items i ON i.id = d.item_id
  LEFT JOIN memory.knowledge_statements st ON st.id = d.statement_id
 WHERE d.application = 'proposed' AND d.carrier_reference LIKE 'artifact:%'
   AND (d.item_id IS NULL OR d.statement_id IS NULL OR st.state = 'gone'
        OR EXISTS (
            SELECT 1
              FROM memory.knowledge_artifact_twins(ARRAY[i.source_id]) t
              JOIN memory.knowledge_items ti ON ti.id = t.twin_item_id
              JOIN memory.knowledge_revisions r ON r.id = ti.source_id
              JOIN memory.knowledge_documents dd ON dd.document_uuid = r.document_uuid
              JOIN memory.knowledge_sources s ON s.source_uuid = dd.source_uuid
              LEFT JOIN projects.locations loc ON loc.location_uuid = s.location_uuid
             WHERE COALESCE(s.machine_uuid, loc.machine_uuid) = $1))
 ORDER BY d.id
 LIMIT $2
