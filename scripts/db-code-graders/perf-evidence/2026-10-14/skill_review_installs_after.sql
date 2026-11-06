INSERT INTO memory.skill_installs AS si
                          (machine_uuid, project_id, folder, machine_name, skill_path, draft_id,
                           lesson_watermark, first_written, retired_at, retired_reason, reported_at)
                   SELECT '00000000-0000-4000-8000-000000000001', p.id, r.folder, 'omit', r.path, d.id, r.lesson_watermark,
                          r.first_written, r.retired_at, r.retired_reason, now()
                     FROM jsonb_to_recordset((select jsonb_agg(jsonb_build_object('project_uuid',p.project_uuid,'folder',i.folder,'path',i.skill_path,'draft_id',i.draft_id,'lesson_watermark',i.lesson_watermark,'first_written',i.first_written,'retired_at',null,'retired_reason',null)) from memory.skill_installs i join projects.projects p on p.id=i.project_id))
                          AS r(project_uuid uuid, folder text, path text, draft_id bigint,
                               lesson_watermark bigint, first_written date, retired_at date,
                               retired_reason text)
                     JOIN projects.projects p ON p.project_uuid = r.project_uuid
                     LEFT JOIN memory.skill_drafts d ON d.id = r.draft_id
                   ON CONFLICT (machine_uuid, project_id, folder) DO UPDATE
                      SET machine_name = EXCLUDED.machine_name, skill_path = EXCLUDED.skill_path,
                          draft_id = EXCLUDED.draft_id, lesson_watermark = EXCLUDED.lesson_watermark,
                          first_written = EXCLUDED.first_written, retired_at = EXCLUDED.retired_at,
                          retired_reason = EXCLUDED.retired_reason,
                          reported_at = EXCLUDED.reported_at
