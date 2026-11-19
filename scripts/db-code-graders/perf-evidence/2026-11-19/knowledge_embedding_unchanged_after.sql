UPDATE memory.knowledge_embedding_state s
   SET reference = 'note:ea77ec40-8977-417a-9c77-2ab7fa7bb289', project_id = 1
 WHERE s.item_id = 76272
   AND s.model_key = '3cc377ac5c4ebbef80db7e5fb3ee26539987cad0f03cb59fbdffad627d2e2523'
   AND s.source_hash = 'sha256:x'
   AND s.completed_at IS NOT NULL
   AND EXISTS (SELECT 1 FROM memory.knowledge_embedding_chunks c WHERE c.state_id = s.id)
RETURNING s.id
