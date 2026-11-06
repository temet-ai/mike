SELECT d.folder, d.description, d.skill_md
  FROM memory.skill_drafts d
 WHERE d.project_id = 993478
   AND d.folder = ANY(ARRAY['fleet-seat-placement-fleet-coordinator', 'fleet-seat-placement'])
   AND d.id < 79
   AND ((d.status = 'accepted' AND d.review IS NULL) OR d.review = 'approved')
 ORDER BY d.lesson_watermark DESC, d.revision DESC
 LIMIT 1
