SELECT p.project_path AS project,st.code AS stage,src.code AS source,
          count(*) AS lessons,count(*) FILTER(WHERE l.embedding IS NULL) AS pending_embeddings,
          min(l.created_at) FILTER(WHERE l.embedding IS NULL) AS oldest_pending
          FROM memory.lessons l JOIN projects.projects p ON p.id=l.project_id
          JOIN memory.lesson_stages st ON st.id=l.stage_id
          JOIN memory.lesson_sources src ON src.id=l.source_id
          WHERE l.archived_at IS NULL GROUP BY 1,2,3 ORDER BY 1,2,3