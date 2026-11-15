SELECT p.project_uuid, d.id, d.folder, d.lesson_watermark, d.revision, d.review,
       d.skill_md, d.reference_files, d.lesson_ids
  FROM memory.skill_drafts d
  JOIN projects.projects p ON p.id = d.project_id
 WHERE (d.status = 'accepted' AND d.review IS NULL) OR d.review IN ('approved', 'rejected')
 ORDER BY d.project_id, d.folder, d.lesson_watermark DESC, d.revision DESC
